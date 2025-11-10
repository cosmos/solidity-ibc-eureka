package main

import (
	"context"
	"encoding/binary"
	"encoding/hex"
	"fmt"
	"os"
	"testing"
	"time"

	gmp_counter_app "github.com/cosmos/solidity-ibc-eureka/e2e/interchaintestv8/solana/go-anchor/gmpcounter"
	bin "github.com/gagliardetto/binary"
	"github.com/stretchr/testify/suite"

	solanago "github.com/gagliardetto/solana-go"
	"github.com/gagliardetto/solana-go/rpc"

	sdkmath "cosmossdk.io/math"

	sdk "github.com/cosmos/cosmos-sdk/types"
	banktypes "github.com/cosmos/cosmos-sdk/x/bank/types"

	transfertypes "github.com/cosmos/ibc-go/v10/modules/apps/transfer/types"
	ibcclientutils "github.com/cosmos/ibc-go/v10/modules/core/02-client/client/utils"
	clienttypesv2 "github.com/cosmos/ibc-go/v10/modules/core/02-client/v2/types"
	channeltypesv2 "github.com/cosmos/ibc-go/v10/modules/core/04-channel/v2/types"
	ibcexported "github.com/cosmos/ibc-go/v10/modules/core/exported"
	tmclient "github.com/cosmos/ibc-go/v10/modules/light-clients/07-tendermint"

	dummy_ibc_app "github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/dummyibcapp"
	ics07_tendermint "github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/ics07tendermint"
	ics26_router "github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/ics26router"
	ics27_gmp "github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/ics27gmp"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/e2esuite"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/relayer"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/solana"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
	relayertypes "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/relayer"
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

	RelayerClient       relayertypes.RelayerServiceClient
	ICS27GMPProgramID   solanago.PublicKey
	GMPCounterProgramID solanago.PublicKey
	DummyAppProgramID   solanago.PublicKey

	// Mock configuration for tests
	UseMockWasmClient bool

	// ALT configuration - if set, will be used when starting relayer
	SolanaAltAddress string
	RelayerProcess   *os.Process
}

func TestWithIbcEurekaSolanaTestSuite(t *testing.T) {
	suite.Run(t, new(IbcEurekaSolanaTestSuite))
}

func (s *IbcEurekaSolanaTestSuite) TearDownSuite() {
	// Clean up relayer process if it's running
	if s.RelayerProcess != nil {
		s.T().Logf("Cleaning up relayer process (PID: %d)", s.RelayerProcess.Pid)
		err := s.RelayerProcess.Kill()
		if err != nil {
			s.T().Logf("Failed to kill relayer process: %v", err)
		}
	}
}

func (s *IbcEurekaSolanaTestSuite) SetupSuite(ctx context.Context) {
	var err error

	err = os.Chdir("../..")
	s.Require().NoError(err)

	os.Setenv(testvalues.EnvKeyEthTestnetType, testvalues.EthTestnetTypeNone)
	os.Setenv(testvalues.EnvKeySolanaTestnetType, testvalues.SolanaTestnetType_Localnet)
	s.TestSuite.SetupSuite(ctx)

	s.T().Log("Waiting for Solana cluster to be ready...")
	err = s.SolanaChain.WaitForClusterReady(ctx, 30*time.Second)
	s.Require().NoError(err, "Solana cluster failed to initialize")

	simd := s.CosmosChains[0]

	s.Require().True(s.Run("Deploy IBC core contracts", func() {
		solanaUser := solanago.NewWallet()
		s.T().Logf("Created SolanaUser wallet: %s", solanaUser.PublicKey())

		fundWallet := func(name string, pubkey solanago.PublicKey, amount uint64) e2esuite.ParallelTask {
			return e2esuite.ParallelTask{
				Name: fmt.Sprintf("Fund %s", name),
				Run: func() error {
					s.T().Logf("Funding %s...", name)
					_, err := s.SolanaChain.FundUserWithRetry(ctx, pubkey, amount, 5)
					if err == nil {
						s.T().Logf("✓ %s funded: %s", name, pubkey)
					}
					return err
				},
			}
		}

		s.Require().True(s.Run("Fund wallets", func() {
			s.T().Log("Funding wallets in parallel...")
			// Fund single deployer with sufficient funds for all program deployments
			const deployerFunding = 100 * testvalues.InitialSolBalance
			err := e2esuite.RunParallelTasks(
				fundWallet("SolanaUser", solanaUser.PublicKey(), testvalues.InitialSolBalance),
				fundWallet("Deployer", solana.DeployerPubkey, deployerFunding),
			)
			s.Require().NoError(err, "Failed to fund wallets")
			s.SolanaUser = solanaUser
			s.T().Log("All wallets funded successfully")
		}))

		s.Require().True(s.Run("Deploy programs", func() {
			// Deploy ALL programs in parallel using single deployer
			s.T().Log("Deploying Solana programs in parallel...")

			const keypairDir = "e2e/interchaintestv8/solana/keypairs"
			const deployerPath = keypairDir + "/deployer_wallet.json"

			deployProgram := func(displayName, programName string) e2esuite.ParallelTaskWithResult[solanago.PublicKey] {
				return e2esuite.ParallelTaskWithResult[solanago.PublicKey]{
					Name: displayName,
					Run: func() (solanago.PublicKey, error) {
						s.T().Logf("Deploying %s...", displayName)
						keypairPath := fmt.Sprintf("%s/%s-keypair.json", keypairDir, programName)
						programID, err := s.SolanaChain.DeploySolanaProgramAsync(ctx, programName, keypairPath, deployerPath)
						if err == nil {
							s.T().Logf("✓ %s deployed at: %s", displayName, programID)
						}
						return programID, err
					},
				}
			}

			deployResults, err := e2esuite.RunParallelTasksWithResults(
				deployProgram("Deploy ICS07 Tendermint", "ics07_tendermint"),
				deployProgram("Deploy ICS26 Router", "ics26_router"),
				deployProgram("Deploy ICS27 GMP", "ics27_gmp"),
				deployProgram("Deploy GMP Counter App", "gmp_counter_app"),
				deployProgram("Deploy Dummy IBC App", "dummy_ibc_app"),
			)
			s.Require().NoError(err, "Program deployment failed")

			ics07_tendermint.ProgramID = deployResults["Deploy ICS07 Tendermint"]
			ics26_router.ProgramID = deployResults["Deploy ICS26 Router"]
			s.ICS27GMPProgramID = deployResults["Deploy ICS27 GMP"]
			ics27_gmp.ProgramID = s.ICS27GMPProgramID
			s.GMPCounterProgramID = deployResults["Deploy GMP Counter App"]
			gmp_counter_app.ProgramID = s.GMPCounterProgramID
			s.DummyAppProgramID = deployResults["Deploy Dummy IBC App"]
			dummy_ibc_app.ProgramID = s.DummyAppProgramID

			s.T().Log("All programs deployed successfully")
		}))
	}))

	s.Require().True(s.Run("Initialize ICS26 Router", func() {
		routerStateAccount, _ := solana.Ics26Router.RouterStatePDA(ics26_router.ProgramID)
		initInstruction, err := ics26_router.NewInitializeInstruction(s.SolanaUser.PublicKey(), routerStateAccount, s.SolanaUser.PublicKey(), solanago.SystemProgramID)
		s.Require().NoError(err)

		tx, err := s.SolanaChain.NewTransactionFromInstructions(s.SolanaUser.PublicKey(), initInstruction)
		s.Require().NoError(err)
		_, err = s.SolanaChain.SignAndBroadcastTxWithRetryAndTimeout(ctx, tx, rpc.CommitmentConfirmed, 30, s.SolanaUser)
		s.Require().NoError(err)
	}))

	s.Require().True(s.Run("Create Address Lookup Table", func() {
		simd := s.CosmosChains[0]
		cosmosChainID := simd.Config().ChainID
		altAddress := s.SolanaChain.CreateIBCAddressLookupTable(ctx, s.T(), s.Require(), s.SolanaUser, cosmosChainID, GMPPortID, SolanaClientID)
		s.SolanaAltAddress = altAddress.String()
		s.T().Logf("Created Address Lookup Table: %s", s.SolanaAltAddress)
	}))

	// Start relayer asynchronously - it can initialize while we set up IBC clients
	type relayerStartResult struct {
		process *os.Process
		err     error
	}
	relayerReady := make(chan relayerStartResult, 1)

	go func() {
		s.T().Log("Starting relayer asynchronously...")

		configInfo := relayer.SolanaCosmosConfigInfo{
			SolanaChainID:        testvalues.SolanaChainID,
			CosmosChainID:        simd.Config().ChainID,
			SolanaRPC:            testvalues.SolanaLocalnetRPC,
			TmRPC:                simd.GetHostRPCAddress(),
			ICS07ProgramID:       ics07_tendermint.ProgramID.String(),
			ICS26RouterProgramID: ics26_router.ProgramID.String(),
			CosmosSignerAddress:  s.CosmosUsers[0].FormattedAddress(),
			SolanaFeePayer:       s.SolanaUser.PublicKey().String(),
			SolanaAltAddress:     s.SolanaAltAddress,
			MockWasmClient:       s.UseMockWasmClient,
		}

		config := relayer.NewConfig(relayer.CreateSolanaCosmosModules(configInfo))

		err := config.GenerateConfigFile(testvalues.RelayerConfigFilePath)
		if err != nil {
			relayerReady <- relayerStartResult{nil, fmt.Errorf("failed to generate config: %w", err)}
			return
		}

		process, err := relayer.StartRelayer(testvalues.RelayerConfigFilePath)
		if err != nil {
			relayerReady <- relayerStartResult{nil, fmt.Errorf("failed to start relayer: %w", err)}
			return
		}

		if s.SolanaAltAddress != "" {
			s.T().Logf("Started relayer with ALT address: %s", s.SolanaAltAddress)
		}

		s.T().Cleanup(func() {
			os.Remove(testvalues.RelayerConfigFilePath)
		})

		relayerReady <- relayerStartResult{process, nil}
		s.T().Log("Relayer startup complete")
	}()

	// Wait for relayer to be ready and create client
	s.Require().True(s.Run("Wait for Relayer and Create Client", func() {
		s.T().Log("Waiting for relayer to be ready...")
		result := <-relayerReady
		s.Require().NoError(result.err, "Relayer failed to start")
		s.RelayerProcess = result.process
		s.T().Log("Relayer is ready")

		// Create relayer gRPC client
		var err error
		s.RelayerClient, err = relayer.GetGRPCClient(relayer.DefaultRelayerGRPCAddress())
		s.Require().NoError(err, "Relayer must be running and accessible")
		s.T().Log("Relayer client created successfully")
	}))

	// Create clients and setup IBC infrastructure
	s.Require().True(s.Run("Setup IBC Clients", func() {
		s.T().Log("Creating IBC clients in parallel...")

		err := e2esuite.RunParallelTasks(
			e2esuite.ParallelTask{
				Name: "Create Tendermint client on Solana",
				Run: func() error {
					s.T().Log("Creating Tendermint Client on Solana...")

					resp, err := s.RelayerClient.CreateClient(context.Background(), &relayertypes.CreateClientRequest{
						SrcChain:   simd.Config().ChainID,
						DstChain:   testvalues.SolanaChainID,
						Parameters: map[string]string{},
					})
					if err != nil {
						return fmt.Errorf("failed to create client tx: %w", err)
					}
					if len(resp.Tx) == 0 {
						return fmt.Errorf("relayer returned empty tx")
					}
					s.T().Logf("Relayer created client transaction")

					unsignedSolanaTx, err := solanago.TransactionFromDecoder(bin.NewBinDecoder(resp.Tx))
					if err != nil {
						return fmt.Errorf("failed to decode tx: %w", err)
					}

					sig, err := s.SolanaChain.SignAndBroadcastTxWithRetry(ctx, unsignedSolanaTx, rpc.CommitmentConfirmed, s.SolanaUser)
					if err != nil {
						return fmt.Errorf("failed to broadcast tx: %w", err)
					}

					s.T().Logf("✓ Tendermint client created on Solana - tx: %s", sig)
					return nil
				},
			},
			e2esuite.ParallelTask{
				Name: "Create WASM client on Cosmos",
				Run: func() error {
					s.T().Log("Creating WASM Client on Cosmos...")

					checksumHex := s.StoreSolanaLightClient(ctx, simd, s.CosmosUsers[0])
					if checksumHex == "" {
						return fmt.Errorf("failed to store Solana light client")
					}

					resp, err := s.RelayerClient.CreateClient(context.Background(), &relayertypes.CreateClientRequest{
						SrcChain: testvalues.SolanaChainID,
						DstChain: simd.Config().ChainID,
						Parameters: map[string]string{
							testvalues.ParameterKey_ChecksumHex: checksumHex,
						},
					})
					if err != nil {
						return fmt.Errorf("failed to create client tx: %w", err)
					}
					if len(resp.Tx) == 0 {
						return fmt.Errorf("relayer returned empty tx")
					}

					txResp := s.MustBroadcastSdkTxBody(ctx, simd, s.CosmosUsers[0], CosmosCreateClientGasLimit, resp.Tx)
					s.T().Logf("✓ WASM client created on Cosmos - tx: %s", txResp.TxHash)
					return nil
				},
			},
		)
		s.Require().NoError(err, "Failed to create IBC clients")
		s.T().Log("Both IBC clients created successfully")

		// Run final setup steps in parallel
		err = e2esuite.NewParallelExecutor().
			Add("Register counterparty on Cosmos", func() error {
				s.T().Log("Registering counterparty on Cosmos chain...")
				merklePathPrefix := [][]byte{[]byte("")}

				_, err := s.BroadcastMessages(ctx, simd, s.CosmosUsers[0], CosmosDefaultGasLimit, &clienttypesv2.MsgRegisterCounterparty{
					ClientId:                 CosmosClientID,
					CounterpartyMerklePrefix: merklePathPrefix,
					CounterpartyClientId:     SolanaClientID,
					Signer:                   s.CosmosUsers[0].FormattedAddress(),
				})
				if err != nil {
					return fmt.Errorf("failed to register counterparty: %w", err)
				}
				s.T().Log("Counterparty registered on Cosmos")
				return nil
			}).
			Add("Add Client to Router on Solana", func() error {
				s.T().Log("Adding client to Router on Solana...")
				routerStateAccount, _ := solana.Ics26Router.RouterStatePDA(ics26_router.ProgramID)
				clientAccount, _ := solana.Ics26Router.ClientPDA(ics26_router.ProgramID, []byte(SolanaClientID))
				clientSequenceAccount, _ := solana.Ics26Router.ClientSequencePDA(ics26_router.ProgramID, []byte(SolanaClientID))

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
				if err != nil {
					return fmt.Errorf("failed to create add client instruction: %w", err)
				}

				tx, err := s.SolanaChain.NewTransactionFromInstructions(s.SolanaUser.PublicKey(), addClientInstruction)
				if err != nil {
					return fmt.Errorf("failed to create transaction: %w", err)
				}

				// Use confirmed commitment - relayer reads Solana state with confirmed commitment
				_, err = s.SolanaChain.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentConfirmed, s.SolanaUser)
				if err != nil {
					return fmt.Errorf("failed to broadcast tx: %w", err)
				}
				s.T().Logf("Client added to router")
				return nil
			}).
			Run()
		s.Require().NoError(err)
	}))
}

// Tests
func (s *IbcEurekaSolanaTestSuite) Test_Deploy() {
	ctx := context.Background()

	s.UseMockWasmClient = true

	s.SetupSuite(ctx)

	simd := s.CosmosChains[0]

	s.Require().True(s.Run("Verify ics07-svm-tendermint", func() {
		clientStateAccount, _ := solana.Ics07Tendermint.ClientPDA(ics07_tendermint.ProgramID, []byte(simd.Config().ChainID))

		// Use confirmed commitment to match client creation confirmation level
		accountInfo, err := s.SolanaChain.RPCClient.GetAccountInfoWithOpts(ctx, clientStateAccount, &rpc.GetAccountInfoOpts{
			Commitment: rpc.CommitmentConfirmed,
		})
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

func (s *IbcEurekaSolanaTestSuite) setupDummyApp(ctx context.Context) {
	s.Require().True(s.Run("Initialize Dummy IBC App", func() {
		appStateAccount, _ := solana.DummyIbcApp.AppStateTransferPDA(s.DummyAppProgramID)

		initInstruction, err := dummy_ibc_app.NewInitializeInstruction(
			s.SolanaUser.PublicKey(),
			appStateAccount,
			s.SolanaUser.PublicKey(),
			solanago.SystemProgramID,
		)
		s.Require().NoError(err)

		tx, err := s.SolanaChain.NewTransactionFromInstructions(s.SolanaUser.PublicKey(), initInstruction)
		s.Require().NoError(err)

		_, err = s.SolanaChain.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentConfirmed, s.SolanaUser)
		s.Require().NoError(err)
		s.T().Logf("Dummy app initialized at: %s", s.DummyAppProgramID)
	}))

	s.Require().True(s.Run("Register Dummy App with Router", func() {
		routerStateAccount, _ := solana.Ics26Router.RouterStatePDA(ics26_router.ProgramID)

		ibcAppAccount, _ := solana.Ics26Router.IbcAppPDA(ics26_router.ProgramID, []byte(transfertypes.PortID))

		registerInstruction, err := ics26_router.NewAddIbcAppInstruction(
			transfertypes.PortID,
			routerStateAccount,
			ibcAppAccount,
			s.DummyAppProgramID,
			s.SolanaUser.PublicKey(),
			s.SolanaUser.PublicKey(),
			solanago.SystemProgramID,
		)
		s.Require().NoError(err)

		tx, err := s.SolanaChain.NewTransactionFromInstructions(s.SolanaUser.PublicKey(), registerInstruction)
		s.Require().NoError(err)

		_, err = s.SolanaChain.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentConfirmed, s.SolanaUser)
		s.Require().NoError(err)
		s.T().Logf("Dummy app registered with router on transfer port")
	}))
}

func (s *IbcEurekaSolanaTestSuite) Test_SolanaToCosmosTransfer_SendPacket() {
	ctx := context.Background()

	s.UseMockWasmClient = true

	s.SetupSuite(ctx)
	s.setupDummyApp(ctx)

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

		var appState, routerCaller, routerState, ibcApp, client, clientSequence, packetCommitment solanago.PublicKey
		s.Require().True(s.Run("Prepare accounts", func() {
			appState, _ = solana.DummyIbcApp.AppStateTransferPDA(s.DummyAppProgramID)
			routerCaller, _ = solana.DummyIbcApp.RouterCallerPDA(s.DummyAppProgramID)
			routerState, _ = solana.Ics26Router.RouterStatePDA(ics26_router.ProgramID)
			ibcApp, _ = solana.Ics26Router.IbcAppPDA(ics26_router.ProgramID, []byte(transfertypes.PortID))
			client, _ = solana.Ics26Router.ClientPDA(ics26_router.ProgramID, []byte(SolanaClientID))
			clientSequence, _ = solana.Ics26Router.ClientSequencePDA(ics26_router.ProgramID, []byte(SolanaClientID))

			// Use confirmed commitment to match overall test commitment level
			clientSequenceAccountInfo, err := s.SolanaChain.RPCClient.GetAccountInfoWithOpts(ctx, clientSequence, &rpc.GetAccountInfoOpts{
				Commitment: rpc.CommitmentConfirmed,
			})
			s.Require().NoError(err)

			clientSequenceData, err := ics26_router.ParseAccount_ClientSequence(clientSequenceAccountInfo.Value.Data.GetBinary())
			s.Require().NoError(err)

			nextSequence := clientSequenceData.NextSequenceSend
			nextSequenceBytes := make([]byte, 8)
			binary.LittleEndian.PutUint64(nextSequenceBytes, nextSequence)
			packetCommitment, _ = solana.Ics26Router.PacketCommitmentPDA(ics26_router.ProgramID, []byte(SolanaClientID), nextSequenceBytes)
		}))

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
			appState,
			s.SolanaUser.PublicKey(),
			routerState,
			ibcApp,
			clientSequence,
			packetCommitment,
			client,
			ics26_router.ProgramID,
			solanago.SystemProgramID,
			routerCaller,
		)
		s.Require().NoError(err)

		tx, err := s.SolanaChain.NewTransactionFromInstructions(s.SolanaUser.PublicKey(), sendPacketInstruction)
		s.Require().NoError(err)

		solanaTxSig, err = s.SolanaChain.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentConfirmed, s.SolanaUser)
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
			s.Require().NotEmpty(resp.Tx, "Relayer Update client should return transaction")

			s.SolanaChain.SubmitChunkedUpdateClient(ctx, s.T(), s.Require(), resp, s.SolanaUser)
			s.Require().NoError(err, "Failed to submit chunked update client transactions")
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
			s.Require().NotEmpty(resp.Tx, "Relay should return transaction")

			_, err = s.SolanaChain.SubmitChunkedRelayPackets(ctx, s.T(), resp, s.SolanaUser)
			s.Require().NoError(err)

			s.SolanaChain.VerifyPacketCommitmentDeleted(ctx, s.T(), s.Require(), SolanaClientID, 1)
		}))
	}))
}

func (s *IbcEurekaSolanaTestSuite) Test_SolanaToCosmosTransfer_SendTransfer() {
	ctx := context.Background()

	s.UseMockWasmClient = true

	s.SetupSuite(ctx)
	s.setupDummyApp(ctx)

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

		var appState, routerCaller, routerState, ibcApp, client, clientSequence, packetCommitment, escrow, escrowState solanago.PublicKey
		s.Require().True(s.Run("Prepare accounts", func() {
			appState, _ = solana.DummyIbcApp.AppStateTransferPDA(s.DummyAppProgramID)
			routerCaller, _ = solana.DummyIbcApp.RouterCallerPDA(s.DummyAppProgramID)
			routerState, _ = solana.Ics26Router.RouterStatePDA(ics26_router.ProgramID)
			ibcApp, _ = solana.Ics26Router.IbcAppPDA(ics26_router.ProgramID, []byte(transfertypes.PortID))
			client, _ = solana.Ics26Router.ClientPDA(ics26_router.ProgramID, []byte(SolanaClientID))
			clientSequence, _ = solana.Ics26Router.ClientSequencePDA(ics26_router.ProgramID, []byte(SolanaClientID))

			// Use confirmed commitment to match overall test commitment level
			clientSequenceAccountInfo, err := s.SolanaChain.RPCClient.GetAccountInfoWithOpts(ctx, clientSequence, &rpc.GetAccountInfoOpts{
				Commitment: rpc.CommitmentConfirmed,
			})
			s.Require().NoError(err)

			clientSequenceData, err := ics26_router.ParseAccount_ClientSequence(clientSequenceAccountInfo.Value.Data.GetBinary())
			s.Require().NoError(err)

			nextSequence := clientSequenceData.NextSequenceSend
			nextSequenceBytes := make([]byte, 8)
			binary.LittleEndian.PutUint64(nextSequenceBytes, nextSequence)
			packetCommitment, _ = solana.Ics26Router.PacketCommitmentPDA(ics26_router.ProgramID, []byte(SolanaClientID), nextSequenceBytes)

			escrow, _ = solana.DummyIbcApp.EscrowPDA(s.DummyAppProgramID, []byte(SolanaClientID))
			escrowState, _ = solana.DummyIbcApp.EscrowStatePDA(s.DummyAppProgramID, []byte(SolanaClientID))
		}))

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
			appState,
			s.SolanaUser.PublicKey(),
			escrow,
			escrowState,
			routerState,
			ibcApp,
			clientSequence,
			packetCommitment,
			client,
			ics26_router.ProgramID,
			solanago.SystemProgramID,
			routerCaller,
		)
		s.Require().NoError(err)

		computeBudgetInstruction := solana.NewComputeBudgetInstruction(400000)

		tx, err := s.SolanaChain.NewTransactionFromInstructions(
			s.SolanaUser.PublicKey(),
			computeBudgetInstruction,
			sendTransferInstruction,
		)
		s.Require().NoError(err)

		solanaTxSig, err = s.SolanaChain.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentConfirmed, s.SolanaUser)
		s.Require().NoError(err)
		s.T().Logf("Transfer transaction sent: %s", solanaTxSig)

		finalLamports, balanceChanged := s.SolanaChain.WaitForBalanceChange(ctx, s.SolanaUser.PublicKey(), initialLamports)
		s.Require().True(balanceChanged, "Balance should change after transfer")

		s.T().Logf("Final SOL balance: %d lamports", finalLamports)
		s.T().Logf("SOL transferred: %d lamports", initialLamports-finalLamports)

		s.Require().Less(finalLamports, initialLamports, "Balance should decrease after transfer")

		escrowBalance, balanceChanged := s.SolanaChain.WaitForBalanceChange(ctx, escrow, 0)
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
			s.Require().NotEmpty(resp.Tx, "Update client should return transaction")

			s.SolanaChain.SubmitChunkedUpdateClient(ctx, s.T(), s.Require(), resp, s.SolanaUser)
			s.Require().NoError(err, "Failed to submit chunked update client transactions")
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
			s.Require().NotEmpty(resp.Tx, "Relay should return transaction")

			_, err = s.SolanaChain.SubmitChunkedRelayPackets(ctx, s.T(), resp, s.SolanaUser)
			s.Require().NoError(err)

			s.SolanaChain.VerifyPacketCommitmentDeleted(ctx, s.T(), s.Require(), SolanaClientID, 1)
		}))
	}))
}

func (s *IbcEurekaSolanaTestSuite) Test_CosmosToSolanaTransfer() {
	ctx := context.Background()

	s.UseMockWasmClient = true

	s.SetupSuite(ctx)
	s.setupDummyApp(ctx)

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
			s.Require().NotEmpty(resp.Tx, "Relayer Update client should return transaction")

			s.SolanaChain.SubmitChunkedUpdateClient(ctx, s.T(), s.Require(), resp, s.SolanaUser)
			s.Require().NoError(err, "Failed to submit chunked update client transactions")
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
			s.Require().NotEmpty(resp.Tx, "Relay should return transaction")

			solanaRelayTxSig, err = s.SolanaChain.SubmitChunkedRelayPackets(ctx, s.T(), resp, s.SolanaUser)
			s.Require().NoError(err)

			s.SolanaChain.VerifyPacketCommitmentDeleted(ctx, s.T(), s.Require(), SolanaClientID, 1)
		}))
	}))

	s.Require().True(s.Run("Verify packet received on Solana", func() {
		// Check that the dummy app state was updated
		dummyAppStateAccount, _ := solana.DummyIbcApp.AppStateTransferPDA(s.DummyAppProgramID)

		// Use confirmed commitment to match relay transaction confirmation level
		accountInfo, err := s.SolanaChain.RPCClient.GetAccountInfoWithOpts(ctx, dummyAppStateAccount, &rpc.GetAccountInfoOpts{
			Commitment: rpc.CommitmentConfirmed,
		})
		s.Require().NoError(err)
		s.Require().NotNil(accountInfo.Value)

		appState, err := dummy_ibc_app.ParseAccount_DummyIbcAppState(accountInfo.Value.Data.GetBinary())
		s.Require().NoError(err)

		s.Require().Greater(appState.PacketsReceived, uint64(0), "Dummy app should have received at least one packet")
		s.T().Logf("Solana dummy app has received %d packets total", appState.PacketsReceived)

		// Check that packet receipt was written
		clientSequenceAccount, _ := solana.Ics26Router.ClientSequencePDA(ics26_router.ProgramID, []byte(SolanaClientID))

		// Use confirmed commitment to match relay transaction confirmation level
		clientSequenceAccountInfo, err := s.SolanaChain.RPCClient.GetAccountInfoWithOpts(ctx, clientSequenceAccount, &rpc.GetAccountInfoOpts{
			Commitment: rpc.CommitmentConfirmed,
		})
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

// Test_TendermintSubmitMisbehaviour_DoubleSign tests the misbehaviour detection flow
// TODO: This test needs to be implemented with fixture data or synthetic headers
func (s *IbcEurekaSolanaTestSuite) Test_TendermintSubmitMisbehaviour_DoubleSign() {
	s.T().Skip("TODO: Implement with fixture data - requires exact header matching")
	ctx := context.Background()
	s.UseMockWasmClient = true
	s.SetupSuite(ctx)

	simd := s.CosmosChains[0]
	cosmosChainID := simd.Config().ChainID

	clientStatePDA, _, err := solanago.FindProgramAddress(
		[][]byte{
			[]byte("client"),
			[]byte(cosmosChainID),
		},
		ics07_tendermint.ProgramID,
	)
	s.Require().NoError(err)

	s.Require().True(s.Run("Update client to establish first consensus state", func() {
		resp, err := s.RelayerClient.UpdateClient(context.Background(), &relayertypes.UpdateClientRequest{
			SrcChain:    simd.Config().ChainID,
			DstChain:    testvalues.SolanaChainID,
			DstClientId: SolanaClientID,
		})
		s.Require().NoError(err)
		s.Require().NotEmpty(resp.Tx)

		s.SolanaChain.SubmitChunkedUpdateClient(ctx, s.T(), s.Require(), resp, s.SolanaUser)
	}))

	var trustedHeight1 uint64
	var trustedHeader1 tmclient.Header
	s.Require().True(s.Run("Get first trusted consensus state", func() {
		accountInfo, err := s.SolanaChain.RPCClient.GetAccountInfoWithOpts(ctx, clientStatePDA, &rpc.GetAccountInfoOpts{
			Commitment: rpc.CommitmentConfirmed,
		})
		s.Require().NoError(err)

		clientState, err := ics07_tendermint.ParseAccount_ClientState(accountInfo.Value.Data.GetBinary())
		s.Require().NoError(err)

		trustedHeight1 = clientState.LatestHeight.RevisionHeight
		s.T().Logf("First trusted consensus state height: %d", trustedHeight1)

		var latestHeight int64
		trustedHeader1, latestHeight, err = ibcclientutils.QueryTendermintHeader(simd.Validators[0].CliContext())
		s.Require().NoError(err)
		s.Require().NotZero(latestHeight)
	}))

	var misbehaviourBytes []byte
	s.Require().True(s.Run("Create misbehaviour evidence", func() {
		header1 := s.CreateTMClientHeader(
			ctx,
			simd,
			int64(trustedHeight1),
			trustedHeader1.GetTime().Add(time.Minute),
			trustedHeader1,
		)

		misbehaviour := tmclient.Misbehaviour{
			ClientId: SolanaClientID,
			Header1:  &header1,
			Header2:  &trustedHeader1,
		}

		var err error
		misbehaviourBytes, err = simd.Config().EncodingConfig.Codec.Marshal(&misbehaviour)
		s.Require().NoError(err)
		s.T().Logf("Misbehaviour evidence size: %d bytes", len(misbehaviourBytes))
	}))

	const chunkSize = 700
	var chunkPDAs []solanago.PublicKey

	s.Require().True(s.Run("Upload misbehaviour chunks", func() {
		numChunks := (len(misbehaviourBytes) + chunkSize - 1) / chunkSize
		s.T().Logf("Splitting misbehaviour into %d chunks", numChunks)

		for i := 0; i < numChunks; i++ {
			start := i * chunkSize
			end := start + chunkSize
			if end > len(misbehaviourBytes) {
				end = len(misbehaviourBytes)
			}
			chunkData := misbehaviourBytes[start:end]

			chunkPDA, _, err := solanago.FindProgramAddress(
				[][]byte{
					[]byte("misbehaviour_chunk"),
					s.SolanaUser.PublicKey().Bytes(),
					[]byte(cosmosChainID),
					{uint8(i)},
				},
				ics07_tendermint.ProgramID,
			)
			s.Require().NoError(err)
			chunkPDAs = append(chunkPDAs, chunkPDA)

			uploadInstruction, err := ics07_tendermint.NewUploadMisbehaviourChunkInstruction(
				ics07_tendermint.Ics07TendermintTypesUploadMisbehaviourChunkParams{
					ClientId:   cosmosChainID,
					ChunkIndex: uint8(i),
					ChunkData:  chunkData,
				},
				chunkPDA,
				clientStatePDA,
				s.SolanaUser.PublicKey(),
				solanago.SystemProgramID,
			)
			s.Require().NoError(err)

			tx, err := s.SolanaChain.NewTransactionFromInstructions(s.SolanaUser.PublicKey(), uploadInstruction)
			s.Require().NoError(err)

			_, err = s.SolanaChain.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentConfirmed, s.SolanaUser)
			s.Require().NoError(err)
			s.T().Logf("✓ Uploaded chunk %d/%d (%d bytes)", i+1, numChunks, len(chunkData))
		}
	}))

	s.Require().True(s.Run("Assemble and submit misbehaviour", func() {
		heightBytes := make([]byte, 8)
		binary.LittleEndian.PutUint64(heightBytes, trustedHeight1)

		consensusStatePDA, _, err := solanago.FindProgramAddress(
			[][]byte{
				[]byte("consensus_state"),
				clientStatePDA.Bytes(),
				heightBytes,
			},
			ics07_tendermint.ProgramID,
		)
		s.Require().NoError(err)

		s.T().Logf("Using consensus state PDA: %s (height %d) for both headers", consensusStatePDA, trustedHeight1)

		assembleInstruction, err := ics07_tendermint.NewAssembleAndSubmitMisbehaviourInstruction(
			cosmosChainID,
			clientStatePDA,
			consensusStatePDA,
			consensusStatePDA,
			s.SolanaUser.PublicKey(),
		)
		s.Require().NoError(err)

		genericInstruction := assembleInstruction.(*solanago.GenericInstruction)
		for _, chunkPDA := range chunkPDAs {
			genericInstruction.AccountValues = append(genericInstruction.AccountValues, &solanago.AccountMeta{
				PublicKey:  chunkPDA,
				IsWritable: true,
				IsSigner:   false,
			})
		}

		tx, err := s.SolanaChain.NewTransactionFromInstructions(s.SolanaUser.PublicKey(), genericInstruction)
		s.Require().NoError(err)

		_, err = s.SolanaChain.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentConfirmed, s.SolanaUser)
		s.Require().NoError(err)
		s.T().Logf("✓ Misbehaviour assembled and submitted successfully")
	}))

	s.Require().True(s.Run("Verify client is frozen", func() {
		accountInfo, err := s.SolanaChain.RPCClient.GetAccountInfoWithOpts(ctx, clientStatePDA, &rpc.GetAccountInfoOpts{
			Commitment: rpc.CommitmentConfirmed,
		})
		s.Require().NoError(err)

		clientState, err := ics07_tendermint.ParseAccount_ClientState(accountInfo.Value.Data.GetBinary())
		s.Require().NoError(err)

		s.Require().NotEqual(uint64(0), clientState.FrozenHeight.RevisionHeight, "Client should be frozen")
		s.T().Logf("✓ Client frozen at height: %d", clientState.FrozenHeight.RevisionHeight)
	}))
}

func (s *IbcEurekaSolanaTestSuite) Test_CleanupOrphanedMisbehaviourChunks() {
	ctx := context.Background()
	s.UseMockWasmClient = true
	s.SetupSuite(ctx)

	cosmosChainID := s.CosmosChains[0].Config().ChainID

	clientStatePDA, _, err := solanago.FindProgramAddress(
		[][]byte{
			[]byte("client"),
			[]byte(cosmosChainID),
		},
		ics07_tendermint.ProgramID,
	)
	s.Require().NoError(err)

	mockMisbehaviourData := make([]byte, 2100)
	for i := range mockMisbehaviourData {
		mockMisbehaviourData[i] = byte(i % 256)
	}

	const chunkSize = 700
	numChunks := (len(mockMisbehaviourData) + chunkSize - 1) / chunkSize
	var chunkPDAs []solanago.PublicKey

	s.Require().True(s.Run("Upload orphaned misbehaviour chunks", func() {
		s.T().Logf("Uploading %d orphaned misbehaviour chunks", numChunks)

		for i := range numChunks {
			start := i * chunkSize
			end := min(start+chunkSize, len(mockMisbehaviourData))
			chunkData := mockMisbehaviourData[start:end]

			chunkPDA, _, err := solanago.FindProgramAddress(
				[][]byte{
					[]byte("misbehaviour_chunk"),
					s.SolanaUser.PublicKey().Bytes(),
					[]byte(cosmosChainID),
					{uint8(i)},
				},
				ics07_tendermint.ProgramID,
			)
			s.Require().NoError(err)
			chunkPDAs = append(chunkPDAs, chunkPDA)

			uploadInstruction, err := ics07_tendermint.NewUploadMisbehaviourChunkInstruction(
				ics07_tendermint.Ics07TendermintTypesUploadMisbehaviourChunkParams{
					ClientId:   cosmosChainID,
					ChunkIndex: uint8(i),
					ChunkData:  chunkData,
				},
				chunkPDA,
				clientStatePDA,
				s.SolanaUser.PublicKey(),
				solanago.SystemProgramID,
			)
			s.Require().NoError(err)

			tx, err := s.SolanaChain.NewTransactionFromInstructions(s.SolanaUser.PublicKey(), uploadInstruction)
			s.Require().NoError(err)

			_, err = s.SolanaChain.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentConfirmed, s.SolanaUser)
			s.Require().NoError(err)
		}
	}))

	s.Require().True(s.Run("Verify chunks exist", func() {
		for i, chunkPDA := range chunkPDAs {
			info, err := s.SolanaChain.RPCClient.GetAccountInfoWithOpts(ctx, chunkPDA, &rpc.GetAccountInfoOpts{
				Commitment: rpc.CommitmentConfirmed,
			})
			s.Require().NoError(err)
			s.Require().NotNil(info.Value)
			s.Require().Greater(info.Value.Lamports, uint64(0), "Chunk %d should have rent", i)
		}
	}))

	initialBalance, err := s.SolanaChain.RPCClient.GetBalance(ctx, s.SolanaUser.PublicKey(), rpc.CommitmentConfirmed)
	s.Require().NoError(err)

	s.Require().True(s.Run("Cleanup orphaned chunks", func() {
		cleanupInstruction, err := ics07_tendermint.NewCleanupIncompleteMisbehaviourInstruction(
			cosmosChainID,
			s.SolanaUser.PublicKey(),
			clientStatePDA,
			s.SolanaUser.PublicKey(),
		)
		s.Require().NoError(err)

		genericInstruction := cleanupInstruction.(*solanago.GenericInstruction)
		for _, chunkPDA := range chunkPDAs {
			genericInstruction.AccountValues = append(genericInstruction.AccountValues, &solanago.AccountMeta{
				PublicKey:  chunkPDA,
				IsWritable: true,
				IsSigner:   false,
			})
		}

		tx, err := s.SolanaChain.NewTransactionFromInstructions(s.SolanaUser.PublicKey(), genericInstruction)
		s.Require().NoError(err)

		_, err = s.SolanaChain.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentConfirmed, s.SolanaUser)
		s.Require().NoError(err)
	}))

	s.Require().True(s.Run("Verify chunks cleaned up", func() {
		chunks := []struct {
			pda  solanago.PublicKey
			name string
		}{}
		for i, pda := range chunkPDAs {
			chunks = append(chunks, struct {
				pda  solanago.PublicKey
				name string
			}{pda, fmt.Sprintf("Chunk %d", i)})
		}

		for _, chunk := range chunks {
			info, err := s.SolanaChain.RPCClient.GetAccountInfoWithOpts(ctx, chunk.pda, &rpc.GetAccountInfoOpts{
				Commitment: rpc.CommitmentConfirmed,
			})
			if err == nil && info.Value != nil {
				s.Require().Equal(uint64(0), info.Value.Lamports, chunk.name+" should have 0 lamports")
				data := info.Value.Data.GetBinary()
				s.Require().Equal(make([]byte, len(data)), data, chunk.name+" data should be zeroed")
			}
		}
	}))

	s.Require().True(s.Run("Verify rent recovered", func() {
		finalBalance, err := s.SolanaChain.RPCClient.GetBalance(ctx, s.SolanaUser.PublicKey(), rpc.CommitmentConfirmed)
		s.Require().NoError(err)
		s.Require().Greater(finalBalance.Value, initialBalance.Value, "Balance should increase from rent recovery")
	}))
}

// Helpers

func getSolDenomOnCosmos() transfertypes.Denom {
	return transfertypes.NewDenom(SolDenom, transfertypes.NewHop("transfer", CosmosClientID))
}
