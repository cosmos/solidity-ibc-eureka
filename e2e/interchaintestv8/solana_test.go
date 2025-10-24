package main

import (
	"context"
	"encoding/binary"
	"encoding/hex"
	"fmt"
	"os"
	"testing"
	"time"

	"github.com/cosmos/solidity-ibc-eureka/e2e/v8/e2esuite"
	"github.com/cosmos/solidity-ibc-eureka/e2e/v8/relayer"
	"github.com/cosmos/solidity-ibc-eureka/e2e/v8/solana"
	"github.com/cosmos/solidity-ibc-eureka/e2e/v8/testvalues"
	relayertypes "github.com/cosmos/solidity-ibc-eureka/e2e/v8/types/relayer"
	bin "github.com/gagliardetto/binary"
	"github.com/stretchr/testify/suite"

	solanago "github.com/gagliardetto/solana-go"
	"github.com/gagliardetto/solana-go/rpc"

	sdkmath "cosmossdk.io/math"

	sdk "github.com/cosmos/cosmos-sdk/types"
	banktypes "github.com/cosmos/cosmos-sdk/x/bank/types"

	transfertypes "github.com/cosmos/ibc-go/v10/modules/apps/transfer/types"
	clienttypesv2 "github.com/cosmos/ibc-go/v10/modules/core/02-client/v2/types"
	channeltypesv2 "github.com/cosmos/ibc-go/v10/modules/core/04-channel/v2/types"
	ibcexported "github.com/cosmos/ibc-go/v10/modules/core/exported"

	dummy_ibc_app "github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/dummyibcapp"
	ics07_tendermint "github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/ics07tendermint"
	ics26_router "github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/ics26router"
)

const (
	// General
	DefaultTimeoutSeconds = 30
	SolanaClientID        = testvalues.CustomClientID
	CosmosClientID        = testvalues.FirstWasmClientID
	// Transfer App
	OneSolInLamports   = 1_000_000_000            // 1 SOL in lamports
	TestTransferAmount = OneSolInLamports / 1_000 // 0.001 SOL in lamports
	SolDenom           = "sol"
	TransferPortID     = transfertypes.PortID
	// Compute Units
	DefaultComputeUnits = uint32(400_000)
	// Cosmos Gas Limits
	CosmosDefaultGasLimit      = uint64(200_000)
	CosmosCreateClientGasLimit = uint64(20_000_000)
)

type IbcEurekaSolanaTestSuite struct {
	e2esuite.TestSuite

	SolanaUser *solanago.Wallet

	RelayerClient     relayertypes.RelayerServiceClient
	DummyAppProgramID solanago.PublicKey

	// Mock configuration for tests
	UseMockWasmClient bool

	// GMP setup - if true, deploys ICS27 GMP program and creates ALT during setup
	SetupGMP bool

	// Dummy App setup - if true, deploys and registers dummy IBC app during setup
	SetupDummyApp bool

	// ALT configuration - if set, will be used when starting relayer
	SolanaAltAddress string
	RelayerProcess   *os.Process
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
	// Enable test-grace-period feature for faster e2e tests (0 second grace period instead of 24 hours)
	os.Setenv("ANCHOR_E2E_FEATURES", "ics26-router/test-grace-period")
	s.TestSuite.SetupSuite(ctx)

	s.T().Log("Waiting for Solana cluster to be ready...")
	err = s.SolanaChain.WaitForClusterReady(ctx, 30*time.Second)
	s.Require().NoError(err, "Solana cluster failed to initialize")

	s.T().Log("Creating and funding Solana test wallet...")
	s.SolanaUser, err = s.SolanaChain.CreateAndFundWalletWithRetry(ctx, 5)
	s.Require().NoError(err, "Solana create/fund wallet has failed")

	simd := s.CosmosChains[0]

	s.Require().True(s.Run("Deploy IBC core contracts", func() {
		_, err := s.SolanaChain.FundUser(solana.DeployerPubkey, 20*testvalues.InitialSolBalance)
		s.Require().NoError(err, "FundUser user failed")

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

	// Initialize router first (required before GMP/Dummy App can register)
	s.Require().True(s.Run("Initialize ICS26 Router", func() {
		routerStateAccount, _, err := solanago.FindProgramAddress([][]byte{[]byte("router_state")}, ics26_router.ProgramID)
		s.Require().NoError(err, "Could not find router_state")
		initInstruction, err := ics26_router.NewInitializeInstruction(s.SolanaUser.PublicKey(), routerStateAccount, s.SolanaUser.PublicKey(), solanago.SystemProgramID)
		s.Require().NoError(err)

		tx, err := s.SolanaChain.NewTransactionFromInstructions(s.SolanaUser.PublicKey(), initInstruction)
		s.Require().NoError(err)
		_, err = s.SolanaChain.SignAndBroadcastTxWithRetry(ctx, tx, s.SolanaUser)
		s.Require().NoError(err)
	}))

	// Deploy and initialize ICS27 GMP program if SetupGMP is enabled (requires initialized router)
	if s.SetupGMP {
		s.deployAndInitializeICS27GMP(ctx)

		// Create Address Lookup Table after GMP deployment (if not already set)
		if s.SolanaAltAddress == "" {
			s.Require().True(s.Run("Create Address Lookup Table", func() {
				altAddress := s.createAddressLookupTable(ctx)
				s.SolanaAltAddress = altAddress.String()
				s.T().Logf("Created Address Lookup Table: %s", s.SolanaAltAddress)
			}))
		}
	}

	// Deploy and register Dummy App if SetupDummyApp is enabled (requires initialized router)
	if s.SetupDummyApp {
		s.Require().True(s.Run("Deploy and Register Dummy App", func() {
			dummyAppProgramID := s.deploySolanaProgram(ctx, "dummy_ibc_app")
			dummy_ibc_app.ProgramID = dummyAppProgramID

			programAvailable := s.SolanaChain.WaitForProgramAvailabilityWithTimeout(ctx, dummyAppProgramID, 120)
			s.Require().True(programAvailable, "Program failed to become available within timeout")

			appStateAccount, _, err := solanago.FindProgramAddress([][]byte{[]byte("app_state"), []byte(transfertypes.PortID)}, dummyAppProgramID)
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

			_, err = s.SolanaChain.SignAndBroadcastTxWithRetry(ctx, tx, s.SolanaUser)
			s.Require().NoError(err)
			s.T().Logf("Dummy app initialized")

			routerStateAccount, _, err := solanago.FindProgramAddress([][]byte{[]byte("router_state")}, ics26_router.ProgramID)
			s.Require().NoError(err)

			ibcAppAccount, _, err := solanago.FindProgramAddress([][]byte{[]byte("ibc_app"), []byte(transfertypes.PortID)}, ics26_router.ProgramID)
			s.Require().NoError(err)

			registerInstruction, err := ics26_router.NewAddIbcAppInstruction(
				transfertypes.PortID,
				routerStateAccount,
				ibcAppAccount,
				dummyAppProgramID,
				s.SolanaUser.PublicKey(),
				s.SolanaUser.PublicKey(),
				solanago.SystemProgramID,
			)
			s.Require().NoError(err)

			tx2, err := s.SolanaChain.NewTransactionFromInstructions(s.SolanaUser.PublicKey(), registerInstruction)
			s.Require().NoError(err)

			_, err = s.SolanaChain.SignAndBroadcastTxWithRetry(ctx, tx2, s.SolanaUser)
			s.Require().NoError(err)
			s.T().Logf("Registered for transfer port")

			s.DummyAppProgramID = dummyAppProgramID
		}))
	}

	// Start relayer after all infrastructure is set up (including ALT if needed)
	s.Require().True(s.Run("Start Relayer", func() {
		configInfo := relayer.SolanaCosmosConfigInfo{
			SolanaChainID:        testvalues.SolanaChainID,
			CosmosChainID:        simd.Config().ChainID,
			SolanaRPC:            testvalues.SolanaLocalnetRPC,
			TmRPC:                simd.GetHostRPCAddress(),
			ICS07ProgramID:       ics07_tendermint.ProgramID.String(),
			ICS26RouterProgramID: ics26_router.ProgramID.String(),
			CosmosSignerAddress:  s.CosmosUsers[0].FormattedAddress(),
			SolanaFeePayer:       s.SolanaUser.PublicKey().String(),
			SolanaAltAddress:     s.SolanaAltAddress, // Use ALT if set
			MockWasmClient:       s.UseMockWasmClient,
		}

		config := relayer.NewConfig(relayer.CreateSolanaCosmosModules(configInfo))

		err = config.GenerateConfigFile(testvalues.RelayerConfigFilePath)
		s.Require().NoError(err)

		s.RelayerProcess, err = relayer.StartRelayer(testvalues.RelayerConfigFilePath)
		s.Require().NoError(err, "Relayer failed to start")

		if s.SolanaAltAddress != "" {
			s.T().Logf("Started relayer with ALT address: %s", s.SolanaAltAddress)
		}

		s.T().Cleanup(func() {
			os.Remove(testvalues.RelayerConfigFilePath)
		})
	}))

	s.T().Cleanup(func() {
		if s.RelayerProcess != nil {
			err := s.RelayerProcess.Kill()
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

	// Create clients and setup IBC infrastructure
	s.Require().True(s.Run("Setup IBC Clients", func() {
		s.Require().True(s.Run("Create Tendermint Client on Solana", func() {
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

				sig, err := s.SolanaChain.SignAndBroadcastTxWithRetry(ctx, unsignedSolanaTx, s.SolanaUser)
				s.Require().NoError(err)

				s.T().Logf("Create client transaction broadcasted: %s", sig)
			}))
		}))

		s.Require().True(s.Run("Create WASM Client on Cosmos", func() {
			var checksumHex string
			s.Require().True(s.Run("Store Solana Light Client", func() {
				checksumHex = s.StoreSolanaLightClient(ctx, simd, s.CosmosUsers[0])
			}))

			var createClientTxBodyBz []byte
			s.Require().True(s.Run("Retrieve create client tx", func() {
				resp, err := s.RelayerClient.CreateClient(context.Background(), &relayertypes.CreateClientRequest{
					SrcChain: testvalues.SolanaChainID,
					DstChain: simd.Config().ChainID,
					Parameters: map[string]string{
						testvalues.ParameterKey_ChecksumHex: checksumHex,
					},
				})
				s.Require().NoError(err)
				s.Require().NotEmpty(resp.Tx)

				createClientTxBodyBz = resp.Tx
			}))

			s.Require().True(s.Run("Broadcast create client tx on Cosmos", func() {
				resp := s.MustBroadcastSdkTxBody(ctx, simd, s.CosmosUsers[0], CosmosCreateClientGasLimit, createClientTxBodyBz)
				s.T().Logf("WASM client created on Cosmos: %s", resp.TxHash)
			}))
		}))

		s.Require().True(s.Run("Register counterparty on Cosmos chain", func() {
			merklePathPrefix := [][]byte{[]byte("")}

			_, err := s.BroadcastMessages(ctx, simd, s.CosmosUsers[0], CosmosDefaultGasLimit, &clienttypesv2.MsgRegisterCounterparty{
				ClientId:                 CosmosClientID,
				CounterpartyMerklePrefix: merklePathPrefix,
				CounterpartyClientId:     SolanaClientID,
				Signer:                   s.CosmosUsers[0].FormattedAddress(),
			})
			s.Require().NoError(err)
		}))

		s.Require().True(s.Run("Add Client to Router", func() {
			routerStateAccount, _, err := solanago.FindProgramAddress([][]byte{[]byte("router_state")}, ics26_router.ProgramID)
			s.Require().NoError(err)

			clientAccount, _, err := solanago.FindProgramAddress([][]byte{[]byte("client"), []byte(SolanaClientID)}, ics26_router.ProgramID)
			s.Require().NoError(err)

			clientSequenceAccount, _, err := solanago.FindProgramAddress([][]byte{[]byte("client_sequence"), []byte(SolanaClientID)}, ics26_router.ProgramID)
			s.Require().NoError(err)

			counterpartyInfo := ics26_router.CounterpartyInfo{
				ClientId:     CosmosClientID,
				MerklePrefix: [][]byte{[]byte(ibcexported.StoreKey), []byte("")},
			}

			addClientInstruction, err := ics26_router.NewAddClientInstruction(
				SolanaClientID,
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

			_, err = s.SolanaChain.SignAndBroadcastTxWithRetry(ctx, tx, s.SolanaUser)
			s.Require().NoError(err)
			s.T().Logf("Client added to router")
		}))
	}))
}

// Tests
func (s *IbcEurekaSolanaTestSuite) Test_Deploy() {
	ctx := context.Background()

	s.UseMockWasmClient = true

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

func (s *IbcEurekaSolanaTestSuite) Test_SolanaToCosmosTransfer_SendPacket() {
	ctx := context.Background()

	s.UseMockWasmClient = true
	s.SetupDummyApp = true

	s.SetupSuite(ctx)

	simd := s.CosmosChains[0]

	var solanaTxSig solanago.Signature
	var cosmosPacketRelayTxHash []byte

	s.Require().True(s.Run("Send ICS20 transfer using send_packet", func() {
		initialBalance := s.SolanaUser.PublicKey()
		balanceResp, err := s.SolanaChain.RPCClient.GetBalance(ctx, initialBalance, "confirmed")
		s.Require().NoError(err)
		initialLamports := balanceResp.Value

		s.T().Logf("Initial SOL balance: %d lamports", initialLamports)

		cosmosUserWallet := s.CosmosUsers[0]
		receiver := cosmosUserWallet.FormattedAddress()

		transferData := transfertypes.NewFungibleTokenPacketData(
			SolDenom,                              // denom
			fmt.Sprintf("%d", TestTransferAmount), // amount as string
			s.SolanaUser.PublicKey().String(),     // sender
			receiver,                              // receiver
			"Test via send_packet",                // memo
		)
		packetData := transferData.GetBytes()

		accounts := s.preparePacketAccounts(ctx, s.DummyAppProgramID, transfertypes.PortID, SolanaClientID)

		packetMsg := dummy_ibc_app.SendPacketMsg{
			SourceClient:     SolanaClientID,
			SourcePort:       transfertypes.PortID,
			DestPort:         transfertypes.PortID,
			Version:          transfertypes.V1,
			Encoding:         "application/json",
			PacketData:       packetData,
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
			accounts.RouterCaller,
		)
		s.Require().NoError(err)

		tx, err := s.SolanaChain.NewTransactionFromInstructions(s.SolanaUser.PublicKey(), sendPacketInstruction)
		s.Require().NoError(err)

		solanaTxSig, err = s.SolanaChain.SignAndBroadcastTxWithRetry(ctx, tx, s.SolanaUser)
		s.Require().NoError(err)
		s.T().Logf("send_packet transaction: %s", solanaTxSig)
		s.T().Logf("Sent ICS20 transfer packet with %d bytes of data", len(packetData))

		finalBalance, err := s.SolanaChain.RPCClient.GetBalance(ctx, s.SolanaUser.PublicKey(), "confirmed")
		s.Require().NoError(err)
		s.T().Logf("Final SOL balance: %d lamports (change: %d lamports for fees)", finalBalance.Value, initialLamports-finalBalance.Value)
		s.T().Logf("Note: send_packet sends IBC transfer data without local escrow - tokens should be minted on destination")

		s.T().Logf("Solana packet transaction %s ready for relaying", solanaTxSig)
	}))

	s.Require().True(s.Run("Relay acknowledgment back to Cosmos", func() {
		var ackRelayTxBodyBz []byte
		s.Require().True(s.Run("Retrieve relay tx", func() {
			resp, err := s.RelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
				SrcChain:    testvalues.SolanaChainID,
				DstChain:    simd.Config().ChainID,
				SourceTxIds: [][]byte{[]byte(solanaTxSig.String())},
				SrcClientId: SolanaClientID,
				DstClientId: CosmosClientID,
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Tx)
			s.Require().Empty(resp.Address)

			ackRelayTxBodyBz = resp.Tx
			s.T().Logf("Retrieved acknowledgment relay transaction with %d bytes", len(ackRelayTxBodyBz))
		}))

		s.Require().True(s.Run("Broadcast acknowledgment relay tx on Cosmos", func() {
			relayTxResult := s.MustBroadcastSdkTxBody(ctx, simd, s.CosmosUsers[0], 200_000, ackRelayTxBodyBz)
			s.T().Logf("Acknowledgment relay transaction: %s (code: %d, gas: %d)",
				relayTxResult.TxHash, relayTxResult.Code, relayTxResult.GasUsed)

			txResp, err := simd.GetTransaction(relayTxResult.TxHash)
			s.Require().NoError(err)
			s.T().Logf("Transaction events count: %d", len(txResp.Events))

			cosmosPacketRelayTxHashBytes, err := hex.DecodeString(relayTxResult.TxHash)
			s.Require().NoError(err)
			cosmosPacketRelayTxHash = cosmosPacketRelayTxHashBytes
		}))
	}))

	var denomOnCosmos transfertypes.Denom
	s.Require().True(s.Run("Verify balances on Cosmos chain", func() {
		cosmosUserAddress := s.CosmosUsers[0].FormattedAddress()

		denomOnCosmos = getSolDenomOnCosmos()

		allBalancesResp, err := e2esuite.GRPCQuery[banktypes.QueryAllBalancesResponse](ctx, simd, &banktypes.QueryAllBalancesRequest{
			Address: cosmosUserAddress,
		})
		s.Require().NoError(err)
		s.T().Logf("All balances for user %s:", cosmosUserAddress)
		for _, balance := range allBalancesResp.Balances {
			s.T().Logf("  - %s: %s", balance.Denom, balance.Amount.String())
		}

		resp, err := e2esuite.GRPCQuery[banktypes.QueryBalanceResponse](ctx, simd, &banktypes.QueryBalanceRequest{
			Address: cosmosUserAddress,
			Denom:   denomOnCosmos.IBCDenom(),
		})
		s.Require().NoError(err, "Balances query failed")
		s.Require().NotNil(resp.Balance, "Balance should not be nil")
		s.T().Logf("Current balance for %s: %s %s", denomOnCosmos.IBCDenom(), resp.Balance.Amount.String(), resp.Balance.Denom)

		expectedAmount := sdkmath.NewInt(TestTransferAmount)
		s.Require().Equal(expectedAmount, resp.Balance.Amount)
		s.Require().Equal(denomOnCosmos.IBCDenom(), resp.Balance.Denom)
	}))

	s.Require().True(s.Run("Acknowledge packet on Solana", func() {
		s.Require().True(s.Run("Update Tendermint client on Solana via chunks", func() {
			resp, err := s.RelayerClient.UpdateClient(context.Background(), &relayertypes.UpdateClientRequest{
				SrcChain:    simd.Config().ChainID,
				DstChain:    testvalues.SolanaChainID,
				DstClientId: SolanaClientID,
			})
			s.Require().NoError(err, "Relayer Update Client failed")
			s.Require().NotEmpty(resp.Txs, "Relayer Update client should return chunked transactions")

			s.submitChunkedUpdateClient(ctx, resp, s.SolanaUser)
			s.Require().NoError(err, "Failed to submit chunked update client transactions")
			s.T().Logf("Successfully updated Tendermint client on Solana using %d chunked transactions", len(resp.Txs))
		}))

		s.Require().True(s.Run("Relay acknowledgment", func() {
			resp, err := s.RelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
				SrcChain:    simd.Config().ChainID,
				DstChain:    testvalues.SolanaChainID,
				SourceTxIds: [][]byte{cosmosPacketRelayTxHash},
				SrcClientId: CosmosClientID,
				DstClientId: SolanaClientID,
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Txs, "Relay should return chunked transactions")
			s.T().Logf("Retrieved %d relay transactions (chunks + final instructions)", len(resp.Txs))

			_ = s.submitChunkedRelayPackets(ctx, resp, s.SolanaUser)
			s.T().Logf("Successfully relayed acknowledgment to Solana using %d transactions", len(resp.Txs))

			// With deferred cleanup, commitment should still exist after acknowledgment
			s.verifyPacketCommitmentExists(ctx, SolanaClientID, 1)
		}))
	}))
}

func (s *IbcEurekaSolanaTestSuite) Test_SolanaToCosmosTransfer_SendTransfer() {
	ctx := context.Background()

	s.UseMockWasmClient = true
	s.SetupDummyApp = true

	s.SetupSuite(ctx)

	simd := s.CosmosChains[0]

	var solanaTxSig solanago.Signature
	var cosmosRelayTxHash []byte
	s.Require().True(s.Run("Send SOL transfer from Solana", func() {
		initialBalance := s.SolanaUser.PublicKey()
		balanceResp, err := s.SolanaChain.RPCClient.GetBalance(ctx, initialBalance, "confirmed")
		s.Require().NoError(err)
		initialLamports := balanceResp.Value

		s.T().Logf("Initial SOL balance: %d lamports", initialLamports)

		transferAmount := fmt.Sprintf("%d", TestTransferAmount)
		cosmosUserWallet := s.CosmosUsers[0]
		receiver := cosmosUserWallet.FormattedAddress()
		memo := "Test transfer from Solana to Cosmos"

		accounts := s.prepareTransferAccounts(ctx, s.DummyAppProgramID, transfertypes.PortID, SolanaClientID)

		timeoutTimestamp := time.Now().Unix() + 3600

		transferMsg := dummy_ibc_app.SendTransferMsg{
			Denom:            SolDenom,
			Amount:           transferAmount,
			Receiver:         receiver,
			SourceClient:     SolanaClientID,
			DestPort:         transfertypes.PortID,
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
			accounts.RouterCaller,
		)
		s.Require().NoError(err)

		computeBudgetInstruction := solana.NewComputeBudgetInstruction(400000)

		tx, err := s.SolanaChain.NewTransactionFromInstructions(
			s.SolanaUser.PublicKey(),
			computeBudgetInstruction,
			sendTransferInstruction,
		)
		s.Require().NoError(err)

		solanaTxSig, err = s.SolanaChain.SignAndBroadcastTxWithRetry(ctx, tx, s.SolanaUser)
		s.Require().NoError(err)
		s.T().Logf("Transfer transaction sent: %s", solanaTxSig)

		finalLamports, balanceChanged := s.SolanaChain.WaitForBalanceChange(ctx, s.SolanaUser.PublicKey(), initialLamports)
		s.Require().True(balanceChanged, "Balance should change after transfer")

		s.T().Logf("Final SOL balance: %d lamports", finalLamports)
		s.T().Logf("SOL transferred: %d lamports", initialLamports-finalLamports)

		s.Require().Less(finalLamports, initialLamports, "Balance should decrease after transfer")

		escrowBalance, balanceChanged := s.SolanaChain.WaitForBalanceChange(ctx, accounts.Escrow, 0)
		s.Require().True(balanceChanged, "Escrow account should receive SOL")

		s.T().Logf("Escrow account balance: %d lamports", escrowBalance)

		expectedAmount := uint64(TestTransferAmount)
		s.Require().Equal(escrowBalance, expectedAmount,
			"Escrow should contain exactly the transferred amount")

		s.T().Logf("Solana transaction %s ready for relaying to Cosmos", solanaTxSig)
	}))

	s.Require().True(s.Run("Relay transfer to Cosmos", func() {
		var relayTxBodyBz []byte
		s.Require().True(s.Run("Retrieve relay tx", func() {
			resp, err := s.RelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
				SrcChain:    testvalues.SolanaChainID,
				DstChain:    simd.Config().ChainID,
				SourceTxIds: [][]byte{[]byte(solanaTxSig.String())},
				SrcClientId: SolanaClientID,
				DstClientId: CosmosClientID,
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Tx)
			s.Require().Empty(resp.Address)

			relayTxBodyBz = resp.Tx
			s.T().Logf("Retrieved relay transaction with %d bytes", len(relayTxBodyBz))
		}))

		s.Require().True(s.Run("Broadcast relay tx on Cosmos", func() {
			relayTxResult := s.MustBroadcastSdkTxBody(ctx, simd, s.CosmosUsers[0], 200_000, relayTxBodyBz)
			s.T().Logf("Relay transaction: %s (code: %d, gas: %d)",
				relayTxResult.TxHash, relayTxResult.Code, relayTxResult.GasUsed)

			cosmosRelayTxHashBytes, err := hex.DecodeString(relayTxResult.TxHash)
			s.Require().NoError(err)
			cosmosRelayTxHash = cosmosRelayTxHashBytes
		}))
	}))

	s.Require().True(s.Run("Verify transfer completion on Cosmos", func() {
		ibc_sol_denom := getSolDenomOnCosmos()

		cosmosUserAddress := s.CosmosUsers[0].FormattedAddress()
		resp, err := e2esuite.GRPCQuery[banktypes.QueryBalanceResponse](ctx, simd, &banktypes.QueryBalanceRequest{
			Address: cosmosUserAddress,
			Denom:   ibc_sol_denom.IBCDenom(),
		})
		s.Require().NoError(err)
		s.Require().NotNil(resp.Balance)
		s.Require().Equal(sdkmath.NewIntFromUint64(TestTransferAmount), resp.Balance.Amount)
		s.Require().Equal(ibc_sol_denom.IBCDenom(), resp.Balance.Denom)
		s.T().Logf("Verified IBC SOL balance on Cosmos: %s %s", resp.Balance.Amount.String(), resp.Balance.Denom)
	}))

	s.Require().True(s.Run("Acknowledge transfer on Solana", func() {
		s.Require().True(s.Run("Update Tendermint client on Solana via chunks", func() {
			resp, err := s.RelayerClient.UpdateClient(context.Background(), &relayertypes.UpdateClientRequest{
				SrcChain:    simd.Config().ChainID,
				DstChain:    testvalues.SolanaChainID,
				DstClientId: SolanaClientID,
			})
			s.Require().NoError(err, "Relayer failed to generate update txs")
			s.Require().NotEmpty(resp.Txs, "Update client should return chunked transactions")

			s.submitChunkedUpdateClient(ctx, resp, s.SolanaUser)
			s.Require().NoError(err, "Failed to submit chunked update client transactions")
			s.T().Logf("Successfully updated Tendermint client on Solana using %d chunked transactions", len(resp.Txs))
		}))

		s.Require().True(s.Run("Relay acknowledgment", func() {
			resp, err := s.RelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
				SrcChain:    simd.Config().ChainID,
				DstChain:    testvalues.SolanaChainID,
				SourceTxIds: [][]byte{cosmosRelayTxHash},
				SrcClientId: CosmosClientID,
				DstClientId: SolanaClientID,
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Txs, "Relay should return chunked transactions")
			s.T().Logf("Retrieved %d relay transactions (chunks + final instructions)", len(resp.Txs))

			_ = s.submitChunkedRelayPackets(ctx, resp, s.SolanaUser)
			s.T().Logf("Successfully relayed acknowledgment to Solana using %d transactions", len(resp.Txs))

			// With deferred cleanup, commitment should still exist after acknowledgment
			s.verifyPacketCommitmentExists(ctx, SolanaClientID, 1)
		}))
	}))
}

func (s *IbcEurekaSolanaTestSuite) Test_CosmosToSolanaTransfer() {
	ctx := context.Background()

	s.UseMockWasmClient = true
	s.SetupDummyApp = true

	s.SetupSuite(ctx)

	simd := s.CosmosChains[0]

	var cosmosRelayPacketTxHash []byte
	var solanaRelayTxSig solanago.Signature

	s.Require().True(s.Run("Send ICS20 transfer from Cosmos to Solana", func() {
		cosmosUserWallet := s.CosmosUsers[0]
		cosmosUserAddress := cosmosUserWallet.FormattedAddress()
		solanaUserAddress := s.SolanaUser.PublicKey().String()
		transferCoin := sdk.NewCoin(simd.Config().Denom, sdkmath.NewInt(TestTransferAmount))

		var initialBalance int64
		s.Require().True(s.Run("Verify balances on Cosmos before transfer", func() {
			initialResp, err := e2esuite.GRPCQuery[banktypes.QueryBalanceResponse](ctx, simd, &banktypes.QueryBalanceRequest{
				Address: cosmosUserAddress,
				Denom:   transferCoin.Denom,
			})
			s.Require().NoError(err)
			s.Require().NotNil(initialResp.Balance)
			initialBalance = initialResp.Balance.Amount.Int64()
			s.T().Logf("Initial Cosmos balance: %d %s", initialBalance, transferCoin.Denom)
		}))

		s.Require().True(s.Run("Send transfer packet from Cosmos", func() {
			timeout := uint64(time.Now().Add(30 * time.Minute).Unix())

			transferPayload := transfertypes.FungibleTokenPacketData{
				Denom:    transferCoin.Denom,
				Amount:   transferCoin.Amount.String(),
				Sender:   cosmosUserAddress,
				Receiver: solanaUserAddress,
				Memo:     "cosmos-to-solana-transfer",
			}
			encodedPayload, err := transfertypes.MarshalPacketData(transferPayload, transfertypes.V1, transfertypes.EncodingProtobuf)
			s.Require().NoError(err)

			payload := channeltypesv2.Payload{
				SourcePort:      transfertypes.PortID,
				DestinationPort: transfertypes.PortID,
				Version:         transfertypes.V1,
				Encoding:        transfertypes.EncodingProtobuf,
				Value:           encodedPayload,
			}
			msgSendPacket := channeltypesv2.MsgSendPacket{
				SourceClient:     CosmosClientID,
				TimeoutTimestamp: timeout,
				Payloads: []channeltypesv2.Payload{
					payload,
				},
				Signer: cosmosUserWallet.FormattedAddress(),
			}

			resp, err := s.BroadcastMessages(ctx, simd, cosmosUserWallet, 200_000, &msgSendPacket)
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.TxHash)

			cosmosPacketTxHashBytes, err := hex.DecodeString(resp.TxHash)
			s.Require().NoError(err)
			cosmosRelayPacketTxHash = cosmosPacketTxHashBytes

			s.T().Logf("Cosmos packet transaction sent: %s", resp.TxHash)
		}))

		s.Require().True(s.Run("Verify balances on Cosmos after transfer", func() {
			finalResp, err := e2esuite.GRPCQuery[banktypes.QueryBalanceResponse](ctx, simd, &banktypes.QueryBalanceRequest{
				Address: cosmosUserAddress,
				Denom:   transferCoin.Denom,
			})
			s.Require().NoError(err)
			s.Require().NotNil(finalResp.Balance)
			finalBalance := finalResp.Balance.Amount.Int64()
			s.T().Logf("Final Cosmos balance: %d %s (transferred: %d)", finalBalance, transferCoin.Denom, initialBalance-finalBalance)
			s.Require().Equal(initialBalance-TestTransferAmount, finalBalance, "Balance should decrease by transfer amount")
		}))
	}))

	s.Require().True(s.Run("Acknowledge packet on Solana", func() {
		s.Require().True(s.Run("Update Tendermint client on Solana via chunks", func() {
			resp, err := s.RelayerClient.UpdateClient(context.Background(), &relayertypes.UpdateClientRequest{
				SrcChain:    simd.Config().ChainID,
				DstChain:    testvalues.SolanaChainID,
				DstClientId: SolanaClientID,
			})
			s.Require().NoError(err, "Relayer Update Client failed")
			s.Require().NotEmpty(resp.Txs, "Relayer Update client should return chunked transactions")

			s.submitChunkedUpdateClient(ctx, resp, s.SolanaUser)
			s.Require().NoError(err, "Failed to submit chunked update client transactions")
			s.T().Logf("Successfully updated Tendermint client on Solana using %d chunked transactions", len(resp.Txs))
		}))

		s.Require().True(s.Run("Relay acknowledgment", func() {
			resp, err := s.RelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
				SrcChain:    simd.Config().ChainID,
				DstChain:    testvalues.SolanaChainID,
				SourceTxIds: [][]byte{cosmosRelayPacketTxHash},
				SrcClientId: CosmosClientID,
				DstClientId: SolanaClientID,
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Txs, "Relay should return chunked transactions")
			s.T().Logf("Retrieved %d relay transactions (chunks + final instructions)", len(resp.Txs))

			solanaRelayTxSig = s.submitChunkedRelayPackets(ctx, resp, s.SolanaUser)
			s.T().Logf("Successfully relayed acknowledgment to Solana using %d transactions", len(resp.Txs))

			// With deferred cleanup, commitment should still exist after acknowledgment
			s.verifyPacketCommitmentExists(ctx, SolanaClientID, 1)
		}))
	}))

	s.Require().True(s.Run("Verify packet received on Solana", func() {
		// Check that the dummy app state was updated
		dummyAppStateAccount, _, err := solanago.FindProgramAddress([][]byte{[]byte("app_state"), []byte(transfertypes.PortID)}, s.DummyAppProgramID)
		s.Require().NoError(err)

		accountInfo, err := s.SolanaChain.RPCClient.GetAccountInfo(ctx, dummyAppStateAccount)
		s.Require().NoError(err)
		s.Require().NotNil(accountInfo.Value)

		appState, err := dummy_ibc_app.ParseAccount_DummyIbcAppState(accountInfo.Value.Data.GetBinary())
		s.Require().NoError(err)

		s.Require().Greater(appState.PacketsReceived, uint64(0), "Dummy app should have received at least one packet")
		s.T().Logf("Solana dummy app has received %d packets total", appState.PacketsReceived)

		// Check that packet receipt was written
		clientSequenceAccount, _, err := solanago.FindProgramAddress([][]byte{[]byte("client_sequence"), []byte(SolanaClientID)}, ics26_router.ProgramID)
		s.Require().NoError(err)

		clientSequenceAccountInfo, err := s.SolanaChain.RPCClient.GetAccountInfo(ctx, clientSequenceAccount)
		s.Require().NoError(err)

		clientSequenceData, err := ics26_router.ParseAccount_ClientSequence(clientSequenceAccountInfo.Value.Data.GetBinary())
		s.Require().NoError(err)

		s.T().Logf("Solana client sequence - next send: %d",
			clientSequenceData.NextSequenceSend)
		s.Require().Greater(clientSequenceData.NextSequenceSend, uint64(0), "Should have processed packets")
	}))

	s.Require().True(s.Run("Verify balances on Solana", func() {
		s.T().Logf("SKIPPED: Solana balance verification not applicable for dummy IBC app")
		s.T().Logf("The dummy app only processes packets without actual token transfers")
	}))

	s.Require().True(s.Run("Relay acknowledgment back to Cosmos", func() {
		var ackRelayTxBodyBz []byte
		s.Require().True(s.Run("Retrieve relay tx", func() {
			resp, err := s.RelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
				SrcChain:    testvalues.SolanaChainID,
				DstChain:    simd.Config().ChainID,
				SourceTxIds: [][]byte{[]byte(solanaRelayTxSig.String())},
				SrcClientId: SolanaClientID,
				DstClientId: CosmosClientID,
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Tx)
			s.Require().Empty(resp.Address)

			ackRelayTxBodyBz = resp.Tx
			s.T().Logf("Retrieved acknowledgment relay transaction with %d bytes", len(ackRelayTxBodyBz))
		}))

		s.Require().True(s.Run("Broadcast acknowledgment relay tx on Cosmos", func() {
			relayTxResult := s.MustBroadcastSdkTxBody(ctx, simd, s.CosmosUsers[0], 200_000, ackRelayTxBodyBz)
			s.T().Logf("Acknowledgment relay transaction: %s (code: %d, gas: %d)",
				relayTxResult.TxHash, relayTxResult.Code, relayTxResult.GasUsed)
		}))
	}))
}

// Helpers

func (s *IbcEurekaSolanaTestSuite) submitChunkedUpdateClient(ctx context.Context, resp *relayertypes.UpdateClientResponse, user *solanago.Wallet) {
	s.Require().NotEqual(0, len(resp.Txs), "no chunked transactions provided")

	totalStart := time.Now()

	// Transaction structure: [chunk1, chunk2, ..., chunkN, assembly]
	chunkCount := len(resp.Txs) - 1 // Total minus assembly
	s.T().Logf("=== Starting Chunked Update Client ===")
	s.T().Logf("Total transactions: %d (%d chunks + 1 assembly)",
		len(resp.Txs),
		chunkCount)

	chunkStart := 0
	chunkEnd := len(resp.Txs) - 1 // Everything except last (assembly)

	type chunkResult struct {
		index    int
		sig      solanago.Signature
		err      error
		duration time.Duration
	}

	// Submit chunks in parallel
	s.T().Logf("--- Phase 1: Uploading %d chunks in parallel ---", chunkCount)
	chunksStart := time.Now()
	chunkResults := make(chan chunkResult, chunkEnd-chunkStart)

	for i := chunkStart; i < chunkEnd; i++ {
		go func(idx int) {
			chunkTxStart := time.Now()

			// Decode
			tx, err := solanago.TransactionFromDecoder(bin.NewBinDecoder(resp.Txs[idx]))
			if err != nil {
				chunkResults <- chunkResult{
					index:    idx,
					err:      fmt.Errorf("failed to decode chunk %d: %w", idx, err),
					duration: time.Since(chunkTxStart),
				}
				return
			}

			// Sign and broadcast (with processed confirmation for fast feedback)
			sig, err := s.SolanaChain.SignAndBroadcastTxWithOpts(ctx, tx, user, rpc.ConfirmationStatusProcessed)
			chunkDuration := time.Since(chunkTxStart)

			if err != nil {
				chunkResults <- chunkResult{
					index:    idx,
					err:      fmt.Errorf("failed to submit chunk %d: %w", idx, err),
					duration: chunkDuration,
				}
				return
			}

			s.T().Logf("[Chunk %d timing] total duration: %v",
				idx, chunkDuration)

			chunkResults <- chunkResult{
				index:    idx,
				sig:      sig,
				duration: chunkDuration,
			}
		}(i)
	}

	// Collect results from all parallel chunk submissions
	completedChunks := 0
	for i := 0; i < chunkEnd-chunkStart; i++ {
		result := <-chunkResults
		s.Require().NoError(result.err, "Chunk was not submitted")
		completedChunks++
		s.T().Logf("✓ Chunk %d/%d uploaded in %v - tx: %s",
			completedChunks, chunkCount, result.duration, result.sig)
	}
	close(chunkResults)

	chunksTotal := time.Since(chunksStart)
	avgChunkTime := chunksTotal / time.Duration(chunkCount)
	s.T().Logf("--- Phase 1 Complete: All %d chunks uploaded in %v (avg: %v/chunk) ---",
		chunkCount, chunksTotal, avgChunkTime)

	// Submit assembly transaction - must be done last (always the last transaction)
	s.T().Logf("--- Phase 2: Assembling and updating client ---")
	assemblyStart := time.Now()

	tx, err := solanago.TransactionFromDecoder(bin.NewBinDecoder(resp.Txs[len(resp.Txs)-1]))
	s.Require().NoError(err, "Failed to decode assembly tx")

	sig, err := s.SolanaChain.SignAndBroadcastTxWithConfirmedStatus(ctx, tx, user)
	s.Require().NoError(err)

	assemblyDuration := time.Since(assemblyStart)
	s.T().Logf("✓ Assembly transaction completed in %v - tx: %s", assemblyDuration, sig)

	totalDuration := time.Since(totalStart)
	s.T().Logf("=== Chunked Update Client Complete ===")
	s.T().Logf("Total time: %v", totalDuration)
	s.T().Logf("  - Chunk upload phase: %v (%d chunks in parallel)", chunksTotal, chunkCount)
	s.T().Logf("  - Assembly phase: %v", assemblyDuration)
}

func (s *IbcEurekaSolanaTestSuite) submitChunkedRelayPackets(ctx context.Context, resp *relayertypes.RelayByTxResponse, user *solanago.Wallet) solanago.Signature {
	s.Require().NotEqual(0, len(resp.Txs), "no relay transactions provided")

	totalStart := time.Now()
	s.T().Logf("=== Starting Chunked Relay Packets ===")
	s.T().Logf("Total transactions: %d (chunks + final instructions)", len(resp.Txs))

	var lastSig solanago.Signature
	// Submit all transactions sequentially
	// Structure: [packet1_chunk0, packet1_chunk1, ..., packet1_final, packet2_chunk0, ...]
	for i, txBytes := range resp.Txs {
		txStart := time.Now()

		tx, err := solanago.TransactionFromDecoder(bin.NewBinDecoder(txBytes))
		s.Require().NoError(err, "Failed to decode transaction %d", i)

		recent, err := s.SolanaChain.RPCClient.GetLatestBlockhash(ctx, rpc.CommitmentFinalized)
		s.Require().NoError(err, "Failed to get latest blockhash for transaction %d", i)
		tx.Message.RecentBlockhash = recent.Value.Blockhash

		// TODO: We can speed up test by waiting for processed on all chunks and then finalized on relay assemble tx
		sig, err := s.SolanaChain.SignAndBroadcastTx(ctx, tx, user)
		s.Require().NoError(err, "Failed to submit transaction %d", i)

		lastSig = sig
		txDuration := time.Since(txStart)
		s.T().Logf("✓ Transaction %d/%d completed in %v - tx: %s",
			i+1, len(resp.Txs), txDuration, sig)
	}

	totalDuration := time.Since(totalStart)
	avgTxTime := totalDuration / time.Duration(len(resp.Txs))
	s.T().Logf("=== Chunked Relay Packets Complete ===")
	s.T().Logf("Total time: %v for %d transactions (avg: %v/tx)",
		totalDuration, len(resp.Txs), avgTxTime)
	s.T().Logf("NOTE: for simplicity all tx chunks are waiting for finalization and are sent sequentially")
	s.T().Logf("In real use only final packet tx (recv/ack/timeout) needs to be finalized")
	return lastSig
}

// verifyPacketCommitmentExists verifies that a packet commitment still exists (deferred cleanup)
func (s *IbcEurekaSolanaTestSuite) verifyPacketCommitmentExists(ctx context.Context, clientID string, sequence uint64) {
	packetCommitmentPDA, _, err := solanago.FindProgramAddress(
		[][]byte{
			[]byte("packet_commitment"),
			[]byte(clientID),
			binary.LittleEndian.AppendUint64(nil, sequence),
		},
		ics26_router.ProgramID,
	)
	s.Require().NoError(err)

	accountInfo, err := s.SolanaChain.RPCClient.GetAccountInfo(ctx, packetCommitmentPDA)
	s.Require().NoError(err, "Failed to get packet commitment account")
	s.Require().NotNil(accountInfo.Value, "Packet commitment should still exist (deferred cleanup)")
	s.Require().Greater(accountInfo.Value.Lamports, uint64(0), "Packet commitment should have lamports")

	s.T().Logf("✓ Packet commitment exists (deferred cleanup) for client %s, sequence %d with %d lamports",
		clientID, sequence, accountInfo.Value.Lamports)
}

// verifyPacketCommitmentDeleted verifies that a packet commitment has been deleted after cleanup
func (s *IbcEurekaSolanaTestSuite) verifyPacketCommitmentDeleted(ctx context.Context, clientID string, sequence uint64) {
	packetCommitmentPDA, _, err := solanago.FindProgramAddress(
		[][]byte{
			[]byte("packet_commitment"),
			[]byte(clientID),
			binary.LittleEndian.AppendUint64(nil, sequence),
		},
		ics26_router.ProgramID,
	)
	s.Require().NoError(err)

	// Query the account - it should either not exist or have 0 lamports (closed)
	accountInfo, err := s.SolanaChain.RPCClient.GetAccountInfo(ctx, packetCommitmentPDA)
	// The account should either not be found (nil) or have been closed (0 lamports)
	if err != nil {
		// Account not found is expected - commitment was deleted
		s.T().Logf("Packet commitment deleted (account not found) for client %s, sequence %d", clientID, sequence)
		return
	}

	if accountInfo.Value == nil || accountInfo.Value.Lamports == 0 {
		s.T().Logf("Packet commitment deleted (account closed) for client %s, sequence %d", clientID, sequence)
		return
	}

	s.Require().Fail("Packet commitment should have been deleted after cleanup",
		"Account %s still exists with %d lamports", packetCommitmentPDA.String(), accountInfo.Value.Lamports)
}

func getSolDenomOnCosmos() transfertypes.Denom {
	return transfertypes.NewDenom(SolDenom, transfertypes.NewHop("transfer", CosmosClientID))
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

func (s *IbcEurekaSolanaTestSuite) prepareBaseAccounts(ctx context.Context, dummyAppProgramID solanago.PublicKey, port, clientID string) AccountSet {
	accounts := AccountSet{}
	var err error

	accounts.AppState, _, err = solanago.FindProgramAddress([][]byte{[]byte("app_state"), []byte(port)}, dummyAppProgramID)
	s.Require().NoError(err)

	accounts.RouterCaller, _, err = solanago.FindProgramAddress([][]byte{[]byte("router_caller")}, dummyAppProgramID)
	s.Require().NoError(err)

	accounts.RouterState, _, err = solanago.FindProgramAddress([][]byte{[]byte("router_state")}, ics26_router.ProgramID)
	s.Require().NoError(err)

	accounts.IBCApp, _, err = solanago.FindProgramAddress([][]byte{[]byte("ibc_app"), []byte(port)}, ics26_router.ProgramID)
	s.Require().NoError(err)

	accounts.Client, _, err = solanago.FindProgramAddress([][]byte{[]byte("client"), []byte(clientID)}, ics26_router.ProgramID)
	s.Require().NoError(err)

	accounts.ClientSequence, _, err = solanago.FindProgramAddress([][]byte{[]byte("client_sequence"), []byte(clientID)}, ics26_router.ProgramID)
	s.Require().NoError(err)

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

func (s *IbcEurekaSolanaTestSuite) prepareTransferAccounts(ctx context.Context, dummyAppProgramID solanago.PublicKey, port, clientID string) AccountSet {
	accounts := s.prepareBaseAccounts(ctx, dummyAppProgramID, port, clientID)
	var err error

	accounts.Escrow, _, err = solanago.FindProgramAddress([][]byte{[]byte("escrow"), []byte(clientID)}, dummyAppProgramID)
	s.Require().NoError(err)

	accounts.EscrowState, _, err = solanago.FindProgramAddress([][]byte{[]byte("escrow_state"), []byte(clientID)}, dummyAppProgramID)
	s.Require().NoError(err)

	return accounts
}

func (s *IbcEurekaSolanaTestSuite) preparePacketAccounts(ctx context.Context, dummyAppProgramID solanago.PublicKey, port, clientID string) AccountSet {
	return s.prepareBaseAccounts(ctx, dummyAppProgramID, port, clientID)
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
	s.Require().NoError(err, "%s program deployment has failed", programName)
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
