package main

import (
	"context"
	"encoding/binary"
	"fmt"
	"os"
	"testing"
	"time"

	bin "github.com/gagliardetto/binary"
	"github.com/stretchr/testify/suite"

	solanago "github.com/gagliardetto/solana-go"

	ibcexported "github.com/cosmos/ibc-go/v10/modules/core/exported"

	dummy_ibc_app "github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/dummyibcapp"
	ics07_tendermint "github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/ics07tendermint"
	ics26_router "github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/ics26router"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/e2esuite"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/relayer"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/solana"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
	relayertypes "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/relayer"
)

const (
	// TestTransferAmount is the amount of SOL to transfer in the test (0.001 SOL in lamports)
	TestTransferAmount = 1000000
	// DefaultTimeoutSeconds is the default timeout for various operations (program availability, transaction broadcast, balance changes)
	DefaultTimeoutSeconds = 120
)

type IbcEurekaSolanaTestSuite struct {
	e2esuite.TestSuite

	SolanaUser *solanago.Wallet

	// Relayer client for cross-chain packet relay
	RelayerClient relayertypes.RelayerServiceClient
}

// TestWithIbcEurekaTestSuite is the boilerplate code that allows the test suite to be run
func TestWithIbcEurekaSolanaTestSuite(t *testing.T) {
	suite.Run(t, new(IbcEurekaSolanaTestSuite))
}

// SetupSuite calls the underlying IbcEurekaTestSuite's SetupSuite method
// and deploys the IbcEureka contract
func (s *IbcEurekaSolanaTestSuite) SetupSuite(ctx context.Context) {
	var err error

	err = os.Chdir("../..")
	s.Require().NoError(err)

	os.Setenv(testvalues.EnvKeyEthTestnetType, testvalues.EthTestnetTypeNone)
	os.Setenv(testvalues.EnvKeySolanaTestnetType, testvalues.SolanaTestnetType_Localnet)
	s.TestSuite.SetupSuite(ctx)

	simd := s.CosmosChains[0]

	s.SolanaUser, err = s.SolanaChain.CreateAndFundWallet()
	s.Require().NoError(err)

	s.Require().True(s.Run("Deploy contracts", func() {
		_, err := s.SolanaChain.FundUser(solana.DeployerPubkey, 20*testvalues.InitialSolBalance)
		s.Require().NoError(err)

		ics07ProgramID := s.deploySolanaProgram(ctx, "ics07_tendermint")
		s.Require().Equal(ics07_tendermint.ProgramID, ics07ProgramID)

		// Set the program ID in the ics07_tendermint package, in case it is not matched automatically
		ics07_tendermint.ProgramID = ics07ProgramID

		ics26RouterProgramID := s.deploySolanaProgram(ctx, "ics26_router")
		s.Require().Equal(ics26_router.ProgramID, ics26RouterProgramID)

		// Ensure both programs are available before proceeding
		ics07Available := s.waitForProgramAvailability(ctx, ics07_tendermint.ProgramID)
		s.Require().True(ics07Available, "ICS07 program failed to become available")

		ics26Available := s.waitForProgramAvailability(ctx, ics26_router.ProgramID)
		s.Require().True(ics26Available, "ICS26 router program failed to become available")
	}))

	// Start the relayer for cross-chain communication
	var relayerProcess *os.Process
	s.Require().True(s.Run("Start Relayer", func() {
		// Configure relayer for Solana <-> Cosmos communication
		config := relayer.NewConfig(relayer.CreateSolanaCosmosModules(
			relayer.SolanaCosmosConfigInfo{
				SolanaChainID:        testvalues.SolanaChainID,
				CosmosChainID:        simd.Config().ChainID,
				SolanaRPC:            testvalues.SolanaLocalnetRPC,
				TmRPC:                simd.GetHostRPCAddress(),
				ICS07ProgramID:       ics07_tendermint.ProgramID.String(),
				ICS26RouterProgramID: ics26_router.ProgramID.String(),
				CosmosSignerAddress:  s.CosmosUsers[0].FormattedAddress(),
				SolanaFeePayer:       s.SolanaUser.PublicKey().String(),
			}),
		)

		err = config.GenerateConfigFile(testvalues.RelayerConfigFilePath)
		s.Require().NoError(err)

		// Now that config matches the relayer's expectations, we can start it
		relayerProcess, err = relayer.StartRelayer(testvalues.RelayerConfigFilePath)
		s.Require().NoError(err, "Failed to start relayer process - tests cannot proceed without working relayer")
		s.T().Log("Relayer started successfully with Solana modules")
	}))

	s.T().Cleanup(func() {
		if relayerProcess != nil {
			err := relayerProcess.Kill()
			if err != nil {
				s.T().Logf("Failed to kill the relayer process: %v", err)
			}
		}
	})

	s.Require().True(s.Run("Create Relayer Client", func() {
		// Create gRPC client for relayer communication
		var err error
		s.RelayerClient, err = relayer.GetGRPCClient(relayer.DefaultRelayerGRPCAddress())
		s.Require().NoError(err, "Relayer must be running and accessible")
		s.T().Log("Relayer client created successfully")
	}))

	s.Require().True(s.Run("Initialize Contracts", func() {
		s.Require().True(s.Run("Initialize ICS26 Router", func() {
			routerStateAccount, _, err := solanago.FindProgramAddress([][]byte{[]byte("router_state")}, ics26_router.ProgramID)
			s.Require().NoError(err)
			initInstruction, err := ics26_router.NewInitializeInstruction(s.SolanaUser.PublicKey(), routerStateAccount, s.SolanaUser.PublicKey(), solanago.SystemProgramID)
			s.Require().NoError(err)

			tx, err := s.SolanaChain.NewTransactionFromInstructions(s.SolanaUser.PublicKey(), initInstruction)
			s.Require().NoError(err)
			_, err = s.signAndBroadcastTxWithRetry(ctx, tx, s.SolanaUser)
			s.Require().NoError(err)
		}))

		s.Require().True(s.Run("Create Relayer Client", func() {
			var createClientTxBz []byte
			s.Require().True(s.Run("Retrieve create client tx from relayer", func() {
				resp, err := s.RelayerClient.CreateClient(context.Background(), &relayertypes.CreateClientRequest{
					SrcChain:   simd.Config().ChainID,
					DstChain:   testvalues.SolanaChainID,
					Parameters: map[string]string{},
				})
				s.Require().NoError(err)
				s.Require().NotEmpty(resp.Tx)
				s.T().Logf("Relayer created client transaction")

				createClientTxBz = resp.Tx
			}))

			s.Require().True(s.Run("Broadcast CreateClient tx on Solana", func() {
				unsignedSolanaTx, err := solanago.TransactionFromDecoder(bin.NewBinDecoder(createClientTxBz))
				s.Require().NoError(err)

				sig, err := s.signAndBroadcastTxWithRetry(ctx, unsignedSolanaTx, s.SolanaUser)
				s.Require().NoError(err)

				s.T().Logf("Create client transaction broadcasted: %s", sig)
			}))
		}))
	}))
}

func (s *IbcEurekaSolanaTestSuite) Test_Deploy() {
	ctx := context.Background()
	s.SetupSuite(ctx)

	simd := s.CosmosChains[0]

	s.Require().True(s.Run("Verify ics07-svm-tendermint", func() {
		clientStateAccount, _, err := solanago.FindProgramAddress([][]byte{[]byte("client"), []byte(simd.Config().ChainID)}, ics07_tendermint.ProgramID)
		s.Require().NoError(err)

		accountInfo, err := s.SolanaChain.RPCClient.GetAccountInfo(ctx, clientStateAccount)
		s.Require().NoError(err)

		clientState, err := ics07_tendermint.ParseAccount_ClientState(accountInfo.Value.Data.GetBinary())
		s.Require().NoError(err)

		s.Require().Equal(simd.Config().ChainID, clientState.ChainId)
		s.Require().Equal(testvalues.DefaultTrustLevel.Denominator, clientState.TrustLevelDenominator)
		s.Require().Equal(testvalues.DefaultTrustLevel.Numerator, clientState.TrustLevelNumerator)
		s.Require().Equal(uint64(testvalues.DefaultTrustPeriod), clientState.TrustingPeriod)
		s.Require().True(clientState.UnbondingPeriod > clientState.TrustingPeriod)
		s.Require().Equal(uint64(testvalues.DefaultMaxClockDrift), clientState.MaxClockDrift)
		s.Require().Equal(uint64(1), clientState.LatestHeight.RevisionNumber)
		s.Require().Equal(uint64(0), clientState.FrozenHeight.RevisionNumber)
		s.Require().Equal(uint64(0), clientState.FrozenHeight.RevisionHeight)
	}))

	s.Require().True(s.Run("Test Relayer Info", func() {
		if s.RelayerClient == nil {
			s.T().Skip("Relayer client not available, skipping info test")
			return
		}

		// Test relayer info for Solana to Cosmos module
		resp, err := s.RelayerClient.Info(context.Background(), &relayertypes.InfoRequest{
			SrcChain: testvalues.SolanaChainID,
			DstChain: simd.Config().ChainID,
		})
		s.Require().NoError(err)
		s.Require().NotNil(resp)

		s.T().Logf("Relayer Info - Source Chain: %+v", resp.SourceChain)
		s.T().Logf("Relayer Info - Target Chain: %+v", resp.TargetChain)
		s.T().Logf("Relayer Info - Metadata: %+v", resp.Metadata)

		// Verify source chain info (Solana) - must be present
		s.Require().NotNil(resp.SourceChain, "Source chain info must be present")
		s.Require().Equal(testvalues.SolanaChainID, resp.SourceChain.ChainId)

		// Verify target chain info (Cosmos) - must be present
		s.Require().NotNil(resp.TargetChain, "Target chain info must be present")
		s.Require().Equal(simd.Config().ChainID, resp.TargetChain.ChainId)
	}))
}

// Test Solana to Cosmos SOL transfer
func (s *IbcEurekaSolanaTestSuite) Test_SolanaToCosmosTransfer_SendTransfer() {
	ctx := context.Background()
	s.SetupSuite(ctx)

	clientID := testvalues.FirstWasmClientID

	// Deploy and register dummy app using helper
	dummyAppProgramID := s.deployAndRegisterDummyApp(ctx, clientID)

	var solanaTxSig solanago.Signature
	s.Require().True(s.Run("Send SOL transfer from Solana", func() {
		// Get initial SOL balance
		initialBalance := s.SolanaUser.PublicKey()
		balanceResp, err := s.SolanaChain.RPCClient.GetBalance(ctx, initialBalance, "confirmed")
		s.Require().NoError(err)
		initialLamports := balanceResp.Value

		s.T().Logf("Initial SOL balance: %d lamports", initialLamports)

		// Parameters for the transfer
		transferAmount := fmt.Sprintf("%d", TestTransferAmount)
		// Use the actual cosmos user address as the receiver
		cosmosUserWallet := s.CosmosUsers[0]
		receiver := cosmosUserWallet.FormattedAddress()
		destPort := "transfer"
		memo := "Test transfer from Solana to Cosmos"

		// Derive necessary accounts using helper
		accounts := s.prepareTransferAccounts(ctx, dummyAppProgramID, clientID, destPort)

		// Create the send_transfer message
		// Set timeout to 1 hour from now
		timeoutTimestamp := time.Now().Unix() + 3600

		transferMsg := dummy_ibc_app.SendTransferMsg{
			Denom:            "sol",
			Amount:           transferAmount,
			Receiver:         receiver,
			SourceClient:     clientID,
			DestPort:         destPort,
			TimeoutTimestamp: timeoutTimestamp,
			Memo:             memo,
		}

		// Create the send_transfer instruction
		sendTransferInstruction, err := dummy_ibc_app.NewSendTransferInstruction(
			transferMsg,
			accounts.AppState,
			s.SolanaUser.PublicKey(),
			accounts.Escrow,
			accounts.EscrowState,
			accounts.RouterState,
			accounts.IBCApp,
			accounts.ClientSequence,
			accounts.PacketCommitment,
			accounts.Client,
			ics26_router.ProgramID,
			solanago.SystemProgramID,
			solanago.SysVarClockPubkey,
			accounts.RouterCaller,
		)
		s.Require().NoError(err)

		// Send the transaction
		tx, err := s.SolanaChain.NewTransactionFromInstructions(s.SolanaUser.PublicKey(), sendTransferInstruction)
		s.Require().NoError(err)

		solanaTxSig, err = s.signAndBroadcastTxWithRetry(ctx, tx, s.SolanaUser)
		s.Require().NoError(err)
		s.T().Logf("Transfer transaction sent: %s", solanaTxSig)

		// Wait for balance change with timeout
		finalLamports, balanceChanged := s.waitForBalanceChange(ctx, s.SolanaUser.PublicKey(), initialLamports)
		s.Require().True(balanceChanged, "Balance should change after transfer")

		s.T().Logf("Final SOL balance: %d lamports", finalLamports)
		s.T().Logf("SOL transferred: %d lamports", initialLamports-finalLamports)

		// Verify that SOL was deducted (should be more than transferAmount due to fees)
		s.Require().Less(finalLamports, initialLamports, "Balance should decrease after transfer")

		// Verify SOL is held in escrow immediately after transfer
		// Wait for escrow account to receive the transferred SOL (should change from 0)
		escrowBalance, balanceChanged := s.waitForBalanceChange(ctx, accounts.Escrow, 0)
		s.Require().True(balanceChanged, "Escrow account should receive SOL")

		s.T().Logf("Escrow account balance: %d lamports", escrowBalance)

		// The escrow should have exactly the transfer amount
		expectedAmount := uint64(TestTransferAmount)
		s.Require().Equal(escrowBalance, expectedAmount,
			"Escrow should contain exactly the transferred amount")

		// Store the transaction signature for relaying
		s.T().Logf("Solana transaction %s ready for relaying to Cosmos", solanaTxSig)
	}))
}

// waitForProgramAvailability waits for a program to be deployed and available with default timeout
func (s *IbcEurekaSolanaTestSuite) waitForProgramAvailability(ctx context.Context, programID solanago.PublicKey) bool {
	return s.waitForProgramAvailabilityWithTimeout(ctx, programID, DefaultTimeoutSeconds)
}

// waitForProgramAvailabilityWithTimeout waits for a program to be deployed and available with custom timeout
func (s *IbcEurekaSolanaTestSuite) waitForProgramAvailabilityWithTimeout(ctx context.Context, programID solanago.PublicKey, timeoutSeconds int) bool {
	for i := range timeoutSeconds {
		accountInfo, err := s.SolanaChain.RPCClient.GetAccountInfo(ctx, programID)
		if err == nil && accountInfo.Value != nil && accountInfo.Value.Executable {
			s.T().Logf("Program %s is available after %d seconds, owner: %s, executable: %v",
				programID.String(), i+1, accountInfo.Value.Owner.String(), accountInfo.Value.Executable)
			return true
		}
		if i == 0 {
			s.T().Logf("Waiting for program %s to be available...", programID.String())
		}
		time.Sleep(1 * time.Second)
	}

	s.T().Logf("Warning: Program %s still not available after %d seconds", programID.String(), timeoutSeconds)
	return false
}

// signAndBroadcastTxWithRetry attempts to broadcast a transaction.
// If broadcasting fails, it retries every 1 second for up to the default timeout.
func (s *IbcEurekaSolanaTestSuite) signAndBroadcastTxWithRetry(ctx context.Context, tx *solanago.Transaction, wallet *solanago.Wallet) (solanago.Signature, error) {
	var lastErr error
	for i := range DefaultTimeoutSeconds {
		// Try to broadcast the transaction directly
		sig, err := s.SolanaChain.SignAndBroadcastTx(ctx, tx, wallet)
		if err == nil {
			// Transaction succeeded
			if i > 0 {
				s.T().Logf("Transaction succeeded after %d seconds: %s", i, sig)
			}
			return sig, nil
		}

		lastErr = err

		// Always retry - log progress periodically
		if i == 0 {
			s.T().Logf("Transaction failed, retrying (this is common during program deployment)")
		} else if i%15 == 0 { // Log every 15 seconds
			s.T().Logf("Still retrying transaction... (%d seconds elapsed)", i)
		}
		time.Sleep(1 * time.Second)
	}

	s.T().Logf("Transaction broadcast timed out after %d seconds, last error: %v", DefaultTimeoutSeconds, lastErr)
	return solanago.Signature{}, fmt.Errorf("transaction broadcast timed out after %d seconds - program may not be ready", DefaultTimeoutSeconds)
}

// waitForBalanceChange waits for a balance change from initialBalance with 1s intervals and default timeout
func (s *IbcEurekaSolanaTestSuite) waitForBalanceChange(ctx context.Context, account solanago.PublicKey, initialBalance uint64) (uint64, bool) {
	for i := range DefaultTimeoutSeconds {
		balanceResp, err := s.SolanaChain.RPCClient.GetBalance(ctx, account, "confirmed")
		if err == nil && balanceResp.Value != initialBalance {
			s.T().Logf("Balance changed after %d seconds: %d lamports (was %d)", i+1, balanceResp.Value, initialBalance)
			return balanceResp.Value, true
		}
		if i == 0 {
			s.T().Logf("Waiting for balance to change from %d lamports...", initialBalance)
		}
		time.Sleep(1 * time.Second)
	}

	s.T().Logf("Warning: Balance did not change from %d lamports after %d seconds", initialBalance, DefaultTimeoutSeconds)
	return initialBalance, false
}

// Test ICS20 transfer using send_packet (generic packet function)
func (s *IbcEurekaSolanaTestSuite) Test_SolanaToCosmosTransfer_SendPacket() {
	ctx := context.Background()
	s.SetupSuite(ctx)

	clientID := testvalues.FirstWasmClientID

	// Variable to store transaction signature for relaying
	var solanaTxSig solanago.Signature

	// Deploy and register dummy app using helper
	dummyAppProgramID := s.deployAndRegisterDummyApp(ctx, clientID)

	s.Require().True(s.Run("Send ICS20 transfer using send_packet", func() {
		// Get initial balance
		initialBalance := s.SolanaUser.PublicKey()
		balanceResp, err := s.SolanaChain.RPCClient.GetBalance(ctx, initialBalance, "confirmed")
		s.Require().NoError(err)
		initialLamports := balanceResp.Value

		s.T().Logf("Initial SOL balance: %d lamports", initialLamports)

		// Prepare receiver
		cosmosUserWallet := s.CosmosUsers[0]
		receiver := cosmosUserWallet.FormattedAddress()

		// Create ICS20 packet data
		packetData := fmt.Sprintf(
			`{"denom":"sol","amount":"%d","sender":"%s","receiver":"%s","memo":"Test via send_packet"}`,
			TestTransferAmount,
			s.SolanaUser.PublicKey(),
			receiver,
		)

		// Derive necessary accounts using helper
		accounts := s.preparePacketAccounts(ctx, dummyAppProgramID, clientID, "transfer")

		packetMsg := dummy_ibc_app.SendPacketMsg{
			SourceClient:     clientID,
			SourcePort:       "transfer",
			DestPort:         "transfer",
			Version:          "ics20-1",
			Encoding:         "json",
			PacketData:       []byte(packetData),
			TimeoutTimestamp: time.Now().Unix() + 3600,
		}

		sendPacketInstruction, err := dummy_ibc_app.NewSendPacketInstruction(
			packetMsg,
			accounts.AppState,
			s.SolanaUser.PublicKey(),
			accounts.RouterState,
			accounts.IBCApp,
			accounts.ClientSequence,
			accounts.PacketCommitment,
			accounts.Client,
			ics26_router.ProgramID,
			solanago.SystemProgramID,
			solanago.SysVarClockPubkey,
			accounts.RouterCaller,
		)
		s.Require().NoError(err)

		tx, err := s.SolanaChain.NewTransactionFromInstructions(s.SolanaUser.PublicKey(), sendPacketInstruction)
		s.Require().NoError(err)

		solanaTxSig, err = s.signAndBroadcastTxWithRetry(ctx, tx, s.SolanaUser)
		s.Require().NoError(err)
		s.T().Logf("send_packet transaction: %s", solanaTxSig)
		s.T().Logf("Sent ICS20 transfer packet with %d bytes of data", len(packetData))

		// Note: Since send_packet doesn't handle escrow, the SOL is not transferred
		// This demonstrates the difference between send_transfer (with escrow) and send_packet (without escrow)
		finalBalance, err := s.SolanaChain.RPCClient.GetBalance(ctx, s.SolanaUser.PublicKey(), "confirmed")
		s.Require().NoError(err)
		s.T().Logf("Final SOL balance: %d lamports (change: %d lamports for fees)", finalBalance.Value, initialLamports-finalBalance.Value)
		s.T().Logf("Note: SOL not escrowed since send_packet doesn't handle token transfers")

		// Store the transaction signature for potential relaying
		s.T().Logf("Solana packet transaction %s ready for relaying", solanaTxSig)
	}))
}

// deployAndRegisterDummyApp deploys the dummy IBC app and registers it for the transfer port
func (s *IbcEurekaSolanaTestSuite) deployAndRegisterDummyApp(ctx context.Context, clientID string) solanago.PublicKey {
	// Deploy dummy-ibc-app program
	dummyAppProgramID := s.deploySolanaProgram(ctx, "dummy_ibc_app")

	// Set the program ID in the generated bindings
	dummy_ibc_app.ProgramID = dummyAppProgramID

	// Wait for program to be fully deployed
	programAvailable := s.waitForProgramAvailabilityWithTimeout(ctx, dummyAppProgramID, 120)
	s.Require().True(programAvailable, "Program failed to become available within timeout")

	// Initialize dummy app state
	appStateAccount, _, err := solanago.FindProgramAddress([][]byte{[]byte("dummy_app_state")}, dummyAppProgramID)
	s.Require().NoError(err)

	initInstruction, err := dummy_ibc_app.NewInitializeInstruction(
		s.SolanaUser.PublicKey(),
		appStateAccount,
		s.SolanaUser.PublicKey(),
		solanago.SystemProgramID,
	)
	s.Require().NoError(err)

	tx, err := s.SolanaChain.NewTransactionFromInstructions(s.SolanaUser.PublicKey(), initInstruction)
	s.Require().NoError(err)

	_, err = s.signAndBroadcastTxWithRetry(ctx, tx, s.SolanaUser)
	s.Require().NoError(err)
	s.T().Logf("Dummy app initialized")

	// Register for "transfer" port
	routerStateAccount, _, err := solanago.FindProgramAddress([][]byte{[]byte("router_state")}, ics26_router.ProgramID)
	s.Require().NoError(err)

	ibcAppAccount, _, err := solanago.FindProgramAddress([][]byte{[]byte("ibc_app"), []byte("transfer")}, ics26_router.ProgramID)
	s.Require().NoError(err)

	routerCallerAccount, _, err := solanago.FindProgramAddress([][]byte{[]byte("router_caller")}, dummyAppProgramID)
	s.Require().NoError(err)

	registerInstruction, err := ics26_router.NewAddIbcAppInstruction(
		"transfer",
		routerStateAccount,
		ibcAppAccount,
		routerCallerAccount,
		s.SolanaUser.PublicKey(),
		s.SolanaUser.PublicKey(),
		solanago.SystemProgramID,
	)
	s.Require().NoError(err)

	tx2, err := s.SolanaChain.NewTransactionFromInstructions(s.SolanaUser.PublicKey(), registerInstruction)
	s.Require().NoError(err)

	_, err = s.SolanaChain.SignAndBroadcastTx(ctx, tx2, s.SolanaUser)
	s.Require().NoError(err)
	s.T().Logf("Registered for transfer port")

	// Add client to router
	clientAccount, _, err := solanago.FindProgramAddress([][]byte{[]byte("client"), []byte(clientID)}, ics26_router.ProgramID)
	s.Require().NoError(err)

	clientSequenceAccount, _, err := solanago.FindProgramAddress([][]byte{[]byte("client_sequence"), []byte(clientID)}, ics26_router.ProgramID)
	s.Require().NoError(err)

	counterpartyInfo := ics26_router.CounterpartyInfo{
		ClientId:     testvalues.FirstWasmClientID,
		MerklePrefix: [][]byte{[]byte(ibcexported.StoreKey), []byte("")},
	}

	addClientInstruction, err := ics26_router.NewAddClientInstruction(
		clientID,
		counterpartyInfo,
		s.SolanaUser.PublicKey(),
		routerStateAccount,
		clientAccount,
		clientSequenceAccount,
		s.SolanaUser.PublicKey(),
		ics07_tendermint.ProgramID,
		solanago.SystemProgramID,
	)
	s.Require().NoError(err)

	tx3, err := s.SolanaChain.NewTransactionFromInstructions(s.SolanaUser.PublicKey(), addClientInstruction)
	s.Require().NoError(err)

	_, err = s.SolanaChain.SignAndBroadcastTx(ctx, tx3, s.SolanaUser)
	s.Require().NoError(err)
	s.T().Logf("Client added to router")

	return dummyAppProgramID
}

// AccountSet contains commonly used account addresses
type AccountSet struct {
	AppState         solanago.PublicKey
	RouterState      solanago.PublicKey
	IBCApp           solanago.PublicKey
	Client           solanago.PublicKey
	ClientSequence   solanago.PublicKey
	RouterCaller     solanago.PublicKey
	PacketCommitment solanago.PublicKey
	Escrow           solanago.PublicKey
	EscrowState      solanago.PublicKey
}

// prepareBaseAccounts derives common accounts needed for all IBC instructions
func (s *IbcEurekaSolanaTestSuite) prepareBaseAccounts(ctx context.Context, dummyAppProgramID solanago.PublicKey, clientID string, port string) AccountSet {
	accounts := AccountSet{}
	var err error

	// Dummy app accounts
	accounts.AppState, _, err = solanago.FindProgramAddress([][]byte{[]byte("dummy_app_state")}, dummyAppProgramID)
	s.Require().NoError(err)

	accounts.RouterCaller, _, err = solanago.FindProgramAddress([][]byte{[]byte("router_caller")}, dummyAppProgramID)
	s.Require().NoError(err)

	// Router accounts
	accounts.RouterState, _, err = solanago.FindProgramAddress([][]byte{[]byte("router_state")}, ics26_router.ProgramID)
	s.Require().NoError(err)

	accounts.IBCApp, _, err = solanago.FindProgramAddress([][]byte{[]byte("ibc_app"), []byte(port)}, ics26_router.ProgramID)
	s.Require().NoError(err)

	accounts.Client, _, err = solanago.FindProgramAddress([][]byte{[]byte("client"), []byte(clientID)}, ics26_router.ProgramID)
	s.Require().NoError(err)

	accounts.ClientSequence, _, err = solanago.FindProgramAddress([][]byte{[]byte("client_sequence"), []byte(clientID)}, ics26_router.ProgramID)
	s.Require().NoError(err)

	// Get next sequence and derive packet commitment
	clientSequenceAccountInfo, err := s.SolanaChain.RPCClient.GetAccountInfo(ctx, accounts.ClientSequence)
	s.Require().NoError(err)

	clientSequenceData, err := ics26_router.ParseAccount_ClientSequence(clientSequenceAccountInfo.Value.Data.GetBinary())
	s.Require().NoError(err)

	nextSequence := clientSequenceData.NextSequenceSend
	sequenceBytes := uint64ToLeBytes(nextSequence)
	accounts.PacketCommitment, _, err = solanago.FindProgramAddress([][]byte{[]byte("packet_commitment"), []byte(clientID), sequenceBytes}, ics26_router.ProgramID)
	s.Require().NoError(err)

	return accounts
}

// prepareTransferAccounts derives all accounts needed for send_transfer instruction
func (s *IbcEurekaSolanaTestSuite) prepareTransferAccounts(ctx context.Context, dummyAppProgramID solanago.PublicKey, clientID string, port string) AccountSet {
	accounts := s.prepareBaseAccounts(ctx, dummyAppProgramID, clientID, port)
	var err error

	// Add escrow-specific accounts for send_transfer
	accounts.Escrow, _, err = solanago.FindProgramAddress([][]byte{[]byte("escrow"), []byte(clientID)}, dummyAppProgramID)
	s.Require().NoError(err)

	accounts.EscrowState, _, err = solanago.FindProgramAddress([][]byte{[]byte("escrow_state"), []byte(clientID)}, dummyAppProgramID)
	s.Require().NoError(err)

	return accounts
}

// preparePacketAccounts derives accounts needed for send_packet instruction (no escrow)
func (s *IbcEurekaSolanaTestSuite) preparePacketAccounts(ctx context.Context, dummyAppProgramID solanago.PublicKey, clientID string, port string) AccountSet {
	return s.prepareBaseAccounts(ctx, dummyAppProgramID, clientID, port)
}

func uint64ToLeBytes(val uint64) []byte {
	b := make([]byte, 8)
	binary.LittleEndian.PutUint64(b, val)
	return b
}

// deploySolanaProgram is a helper function that standardizes Solana program deployment
func (s *IbcEurekaSolanaTestSuite) deploySolanaProgram(ctx context.Context, programName string) solanago.PublicKey {
	keypairPath := fmt.Sprintf("e2e/interchaintestv8/solana/%s-keypair.json", programName)
	programID, _, err := solana.AnchorDeploy(ctx, "programs/solana", programName, keypairPath)
	s.Require().NoError(err)
	s.T().Logf("%s program deployed at: %s", programName, programID.String())
	return programID
}
