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
	TestTransferAmount    = 1000000 // 0.001 SOL in lamports
	DefaultTimeoutSeconds = 60
)

type IbcEurekaSolanaTestSuite struct {
	e2esuite.TestSuite

	SolanaUser *solanago.Wallet

	RelayerClient     relayertypes.RelayerServiceClient
	DummyAppProgramID solanago.PublicKey
}

func TestWithIbcEurekaSolanaTestSuite(t *testing.T) {
	suite.Run(t, new(IbcEurekaSolanaTestSuite))
}

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

		ics07_tendermint.ProgramID = ics07ProgramID

		ics26RouterProgramID := s.deploySolanaProgram(ctx, "ics26_router")
		s.Require().Equal(ics26_router.ProgramID, ics26RouterProgramID)

		ics07Available := s.waitForProgramAvailability(ctx, ics07_tendermint.ProgramID)
		s.Require().True(ics07Available, "ICS07 program failed to become available")

		ics26Available := s.waitForProgramAvailability(ctx, ics26_router.ProgramID)
		s.Require().True(ics26Available, "ICS26 router program failed to become available")
	}))

	var relayerProcess *os.Process
	s.Require().True(s.Run("Start Relayer", func() {
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

		relayerProcess, err = relayer.StartRelayer(testvalues.RelayerConfigFilePath)
		s.Require().NoError(err)

		s.T().Cleanup(func() {
			os.Remove(testvalues.RelayerConfigFilePath)
		})
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

		s.Require().True(s.Run("Add Client to Router", func() {
			s.addClientToRouter(ctx, simd.Config().ChainID)
		}))

		s.Require().True(s.Run("Deploy and Register Dummy App", func() {
			s.DummyAppProgramID = s.deployAndRegisterDummyApp(ctx)
		}))
	}))
}

// Tests

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

		resp, err := s.RelayerClient.Info(context.Background(), &relayertypes.InfoRequest{
			SrcChain: testvalues.SolanaChainID,
			DstChain: simd.Config().ChainID,
		})
		s.Require().NoError(err)
		s.Require().NotNil(resp)

		s.T().Logf("Relayer Info - Source Chain: %+v", resp.SourceChain)
		s.T().Logf("Relayer Info - Target Chain: %+v", resp.TargetChain)
		s.T().Logf("Relayer Info - Metadata: %+v", resp.Metadata)

		s.Require().NotNil(resp.SourceChain, "Source chain info must be present")
		s.Require().Equal(testvalues.SolanaChainID, resp.SourceChain.ChainId)

		s.Require().NotNil(resp.TargetChain, "Target chain info must be present")
		s.Require().Equal(simd.Config().ChainID, resp.TargetChain.ChainId)
	}))
}

func (s *IbcEurekaSolanaTestSuite) Test_SolanaToCosmosTransfer_SendTransfer() {
	ctx := context.Background()
	s.SetupSuite(ctx)

	simd := s.CosmosChains[0]
	chainID := simd.Config().ChainID

	var solanaTxSig solanago.Signature
	s.Require().True(s.Run("Send SOL transfer from Solana", func() {
		initialBalance := s.SolanaUser.PublicKey()
		balanceResp, err := s.SolanaChain.RPCClient.GetBalance(ctx, initialBalance, "confirmed")
		s.Require().NoError(err)
		initialLamports := balanceResp.Value

		s.T().Logf("Initial SOL balance: %d lamports", initialLamports)

		transferAmount := fmt.Sprintf("%d", TestTransferAmount)
		cosmosUserWallet := s.CosmosUsers[0]
		receiver := cosmosUserWallet.FormattedAddress()
		destPort := "transfer"
		memo := "Test transfer from Solana to Cosmos"

		accounts := s.prepareTransferAccounts(ctx, s.DummyAppProgramID, chainID, destPort)

		timeoutTimestamp := time.Now().Unix() + 3600

		transferMsg := dummy_ibc_app.SendTransferMsg{
			Denom:            "sol",
			Amount:           transferAmount,
			Receiver:         receiver,
			SourceClient:     chainID,
			DestPort:         destPort,
			TimeoutTimestamp: timeoutTimestamp,
			Memo:             memo,
		}

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

		tx, err := s.SolanaChain.NewTransactionFromInstructions(s.SolanaUser.PublicKey(), sendTransferInstruction)
		s.Require().NoError(err)

		solanaTxSig, err = s.signAndBroadcastTxWithRetry(ctx, tx, s.SolanaUser)
		s.Require().NoError(err)
		s.T().Logf("Transfer transaction sent: %s", solanaTxSig)

		finalLamports, balanceChanged := s.waitForBalanceChange(ctx, s.SolanaUser.PublicKey(), initialLamports)
		s.Require().True(balanceChanged, "Balance should change after transfer")

		s.T().Logf("Final SOL balance: %d lamports", finalLamports)
		s.T().Logf("SOL transferred: %d lamports", initialLamports-finalLamports)

		s.Require().Less(finalLamports, initialLamports, "Balance should decrease after transfer")

		escrowBalance, balanceChanged := s.waitForBalanceChange(ctx, accounts.Escrow, 0)
		s.Require().True(balanceChanged, "Escrow account should receive SOL")

		s.T().Logf("Escrow account balance: %d lamports", escrowBalance)

		expectedAmount := uint64(TestTransferAmount)
		s.Require().Equal(escrowBalance, expectedAmount,
			"Escrow should contain exactly the transferred amount")

		s.T().Logf("Solana transaction %s ready for relaying to Cosmos", solanaTxSig)
	}))
}

func (s *IbcEurekaSolanaTestSuite) Test_SolanaToCosmosTransfer_SendPacket() {
	ctx := context.Background()
	s.SetupSuite(ctx)

	simd := s.CosmosChains[0]
	chainID := simd.Config().ChainID

	var solanaTxSig solanago.Signature

	s.Require().True(s.Run("Send ICS20 transfer using send_packet", func() {
		initialBalance := s.SolanaUser.PublicKey()
		balanceResp, err := s.SolanaChain.RPCClient.GetBalance(ctx, initialBalance, "confirmed")
		s.Require().NoError(err)
		initialLamports := balanceResp.Value

		s.T().Logf("Initial SOL balance: %d lamports", initialLamports)

		cosmosUserWallet := s.CosmosUsers[0]
		receiver := cosmosUserWallet.FormattedAddress()

		packetData := fmt.Sprintf(
			`{"denom":"sol","amount":"%d","sender":"%s","receiver":"%s","memo":"Test via send_packet"}`,
			TestTransferAmount,
			s.SolanaUser.PublicKey(),
			receiver,
		)

		accounts := s.preparePacketAccounts(ctx, s.DummyAppProgramID, chainID, "transfer")

		packetMsg := dummy_ibc_app.SendPacketMsg{
			SourceClient:     chainID,
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

		finalBalance, err := s.SolanaChain.RPCClient.GetBalance(ctx, s.SolanaUser.PublicKey(), "confirmed")
		s.Require().NoError(err)
		s.T().Logf("Final SOL balance: %d lamports (change: %d lamports for fees)", finalBalance.Value, initialLamports-finalBalance.Value)
		s.T().Logf("Note: SOL not escrowed since send_packet doesn't handle token transfers")

		s.T().Logf("Solana packet transaction %s ready for relaying", solanaTxSig)
	}))
}

// Helpers

func (s *IbcEurekaSolanaTestSuite) deployAndRegisterDummyApp(ctx context.Context) solanago.PublicKey {
	dummyAppProgramID := s.deploySolanaProgram(ctx, "dummy_ibc_app")

	dummy_ibc_app.ProgramID = dummyAppProgramID

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

	_, err = s.signAndBroadcastTxWithRetry(ctx, tx2, s.SolanaUser)
	s.Require().NoError(err)
	s.T().Logf("Registered for transfer port")

	return dummyAppProgramID
}

func (s *IbcEurekaSolanaTestSuite) addClientToRouter(ctx context.Context, chainID string) {
	routerStateAccount, _, err := solanago.FindProgramAddress([][]byte{[]byte("router_state")}, ics26_router.ProgramID)
	s.Require().NoError(err)

	clientAccount, _, err := solanago.FindProgramAddress([][]byte{[]byte("client"), []byte(chainID)}, ics26_router.ProgramID)
	s.Require().NoError(err)

	clientSequenceAccount, _, err := solanago.FindProgramAddress([][]byte{[]byte("client_sequence"), []byte(chainID)}, ics26_router.ProgramID)
	s.Require().NoError(err)

	counterpartyInfo := ics26_router.CounterpartyInfo{
		ClientId:     testvalues.FirstWasmClientID,
		MerklePrefix: [][]byte{[]byte(ibcexported.StoreKey), []byte("")},
	}

	addClientInstruction, err := ics26_router.NewAddClientInstruction(
		chainID,
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

	tx, err := s.SolanaChain.NewTransactionFromInstructions(s.SolanaUser.PublicKey(), addClientInstruction)
	s.Require().NoError(err)

	_, err = s.signAndBroadcastTxWithRetry(ctx, tx, s.SolanaUser)
	s.Require().NoError(err)
	s.T().Logf("Client added to router")
}

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

func (s *IbcEurekaSolanaTestSuite) prepareBaseAccounts(ctx context.Context, dummyAppProgramID solanago.PublicKey, chainID, port string) AccountSet {
	accounts := AccountSet{}
	var err error

	accounts.AppState, _, err = solanago.FindProgramAddress([][]byte{[]byte("dummy_app_state")}, dummyAppProgramID)
	s.Require().NoError(err)

	accounts.RouterCaller, _, err = solanago.FindProgramAddress([][]byte{[]byte("router_caller")}, dummyAppProgramID)
	s.Require().NoError(err)

	accounts.RouterState, _, err = solanago.FindProgramAddress([][]byte{[]byte("router_state")}, ics26_router.ProgramID)
	s.Require().NoError(err)

	accounts.IBCApp, _, err = solanago.FindProgramAddress([][]byte{[]byte("ibc_app"), []byte(port)}, ics26_router.ProgramID)
	s.Require().NoError(err)

	accounts.Client, _, err = solanago.FindProgramAddress([][]byte{[]byte("client"), []byte(chainID)}, ics26_router.ProgramID)
	s.Require().NoError(err)

	accounts.ClientSequence, _, err = solanago.FindProgramAddress([][]byte{[]byte("client_sequence"), []byte(chainID)}, ics26_router.ProgramID)
	s.Require().NoError(err)

	clientSequenceAccountInfo, err := s.SolanaChain.RPCClient.GetAccountInfo(ctx, accounts.ClientSequence)
	s.Require().NoError(err)

	clientSequenceData, err := ics26_router.ParseAccount_ClientSequence(clientSequenceAccountInfo.Value.Data.GetBinary())
	s.Require().NoError(err)

	nextSequence := clientSequenceData.NextSequenceSend
	sequenceBytes := uint64ToLeBytes(nextSequence)
	accounts.PacketCommitment, _, err = solanago.FindProgramAddress([][]byte{[]byte("packet_commitment"), []byte(chainID), sequenceBytes}, ics26_router.ProgramID)
	s.Require().NoError(err)

	return accounts
}

func (s *IbcEurekaSolanaTestSuite) prepareTransferAccounts(ctx context.Context, dummyAppProgramID solanago.PublicKey, chainID, port string) AccountSet {
	accounts := s.prepareBaseAccounts(ctx, dummyAppProgramID, chainID, port)
	var err error

	accounts.Escrow, _, err = solanago.FindProgramAddress([][]byte{[]byte("escrow"), []byte(chainID)}, dummyAppProgramID)
	s.Require().NoError(err)

	accounts.EscrowState, _, err = solanago.FindProgramAddress([][]byte{[]byte("escrow_state"), []byte(chainID)}, dummyAppProgramID)
	s.Require().NoError(err)

	return accounts
}

func (s *IbcEurekaSolanaTestSuite) preparePacketAccounts(ctx context.Context, dummyAppProgramID solanago.PublicKey, chainID, port string) AccountSet {
	return s.prepareBaseAccounts(ctx, dummyAppProgramID, chainID, port)
}

func uint64ToLeBytes(val uint64) []byte {
	b := make([]byte, 8)
	binary.LittleEndian.PutUint64(b, val)
	return b
}

func (s *IbcEurekaSolanaTestSuite) deploySolanaProgram(ctx context.Context, programName string) solanago.PublicKey {
	keypairPath := fmt.Sprintf("e2e/interchaintestv8/solana/%s-keypair.json", programName)
	walletPath := "e2e/interchaintestv8/solana/deployer_wallet.json"
	programID, _, err := solana.AnchorDeploy(ctx, "programs/solana", programName, keypairPath, walletPath)
	s.Require().NoError(err)
	s.T().Logf("%s program deployed at: %s", programName, programID.String())
	return programID
}

func (s *IbcEurekaSolanaTestSuite) waitForProgramAvailability(ctx context.Context, programID solanago.PublicKey) bool {
	return s.waitForProgramAvailabilityWithTimeout(ctx, programID, DefaultTimeoutSeconds)
}

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

func (s *IbcEurekaSolanaTestSuite) signAndBroadcastTxWithRetry(ctx context.Context, tx *solanago.Transaction, wallet *solanago.Wallet) (solanago.Signature, error) {
	var lastErr error
	for i := range DefaultTimeoutSeconds {
		sig, err := s.SolanaChain.SignAndBroadcastTx(ctx, tx, wallet)
		if err == nil {
			if i > 0 {
				s.T().Logf("Transaction succeeded after %d seconds: %s", i, sig)
			}
			return sig, nil
		}

		lastErr = err

		if i == 0 {
			s.T().Logf("Transaction failed, retrying (this is common during program deployment)")
		} else if i%15 == 0 {
			s.T().Logf("Still retrying transaction... (%d seconds elapsed)", i)
		}
		time.Sleep(1 * time.Second)
	}

	s.T().Logf("Transaction broadcast timed out after %d seconds, last error: %v", DefaultTimeoutSeconds, lastErr)
	return solanago.Signature{}, fmt.Errorf("transaction broadcast timed out after %d seconds - program may not be ready", DefaultTimeoutSeconds)
}

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
