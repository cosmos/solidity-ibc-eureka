package main

import (
	"context"
	"encoding/binary"
	"encoding/hex"
	"fmt"
	"os"
	"strconv"
	"strings"
	"testing"
	"time"

	test_ibc_app "github.com/cosmos/solidity-ibc-eureka/e2e/interchaintestv8/solana/go-anchor/testibcapp"
	bin "github.com/gagliardetto/binary"
	"github.com/stretchr/testify/suite"
	"google.golang.org/protobuf/proto"

	solanago "github.com/gagliardetto/solana-go"
	"github.com/gagliardetto/solana-go/rpc"

	sdkmath "cosmossdk.io/math"

	sdk "github.com/cosmos/cosmos-sdk/types"
	banktypes "github.com/cosmos/cosmos-sdk/x/bank/types"

	transfertypes "github.com/cosmos/ibc-go/v10/modules/apps/transfer/types"
	clienttypesv2 "github.com/cosmos/ibc-go/v10/modules/core/02-client/v2/types"
	channeltypesv2 "github.com/cosmos/ibc-go/v10/modules/core/04-channel/v2/types"

	"github.com/cosmos/interchaintest/v10/testutil"

	ics26router "github.com/cosmos/solidity-ibc-eureka/packages/go-abigen/ics26router"

	access_manager "github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/accessmanager"
	attestation "github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/attestation"
	ics07_tendermint "github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/ics07tendermint"
	ics26_router "github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/ics26router"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/attestor"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/e2esuite"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/relayer"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/solana"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types"
	attestortypes "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/attestor"
	relayertypes "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/relayer"
)

const (
	// Number of Cosmos attestors for Cosmos → Solana direction.
	// Max supported is 11 due to Solana transaction size limit (1232 bytes).
	numCosmosAttestors = 11

	// Number of Solana attestors for Solana → Cosmos direction.
	numSolanaAttestors = 2
)

// IbcSolanaAttestationTestSuite is a comprehensive test suite for Solana attestation flows.
// It tests both directions:
// - Cosmos → Solana: using attestation-light-client with Cosmos attestors
// - Solana → Cosmos: using Solana attestors with mock/wasm client on Cosmos
type IbcSolanaAttestationTestSuite struct {
	e2esuite.TestSuite

	SolanaUser           *solanago.Wallet
	Ics26RouterProgramID solanago.PublicKey
	TestAppProgramID     solanago.PublicKey

	// Light clients on Solana
	Ics07TendermintProgramID        solanago.PublicKey
	AttestationLightClientProgramID solanago.PublicKey

	// AttestationClientID is the IBC client ID for the attestation light client (must be 8-64 chars)
	AttestationClientID string

	SolanaAltAddress string

	// Cosmos attestors (for Cosmos → Solana direction)
	CosmosAttestorContainers []*attestor.AttestorContainer
	CosmosAttestorEndpoints  []string
	CosmosAttestorAddresses  []string

	// Solana attestors (for Solana → Cosmos direction)
	SolanaAttestorContainers []*attestor.AttestorContainer
	SolanaAttestorEndpoints  []string
	SolanaAttestorAddresses  []string
	SolanaAttestorClient     attestortypes.AttestationServiceClient

	RelayerClient  relayertypes.RelayerServiceClient
	RelayerProcess *os.Process
}

func TestWithIbcSolanaAttestationTestSuite(t *testing.T) {
	suite.Run(t, new(IbcSolanaAttestationTestSuite))
}

func (s *IbcSolanaAttestationTestSuite) TearDownSuite() {
	ctx := context.Background()
	attestor.CleanupContainers(ctx, s.T(), s.CosmosAttestorContainers)
	attestor.CleanupContainers(ctx, s.T(), s.SolanaAttestorContainers)

	if s.RelayerProcess != nil {
		s.T().Logf("Cleaning up relayer process (PID: %d)", s.RelayerProcess.Pid)
		if err := s.RelayerProcess.Kill(); err != nil {
			s.T().Logf("Failed to kill relayer process: %v", err)
		}
	}
}

func (s *IbcSolanaAttestationTestSuite) SetupSuite(ctx context.Context) {
	var err error

	err = os.Chdir("../..")
	s.Require().NoError(err)

	os.Setenv(testvalues.EnvKeyEthTestnetType, testvalues.EthTestnetType_None)
	os.Setenv(testvalues.EnvKeySolanaTestnetType, testvalues.SolanaTestnetType_Localnet)
	os.Setenv(testvalues.EnvKeyEthLcOnCosmos, testvalues.EthWasmTypeAttestorWasm)

	s.TestSuite.SetupSuite(ctx)

	err = s.Solana.Chain.WaitForClusterReady(ctx, 30*time.Second)
	s.Require().NoError(err)

	s.SolanaUser = solanago.NewWallet()

	_, err = s.Solana.Chain.FundUserWithRetry(ctx, s.SolanaUser.PublicKey(), testvalues.InitialSolBalance, 5)
	s.Require().NoError(err)

	const deployerFunding = 100 * testvalues.InitialSolBalance
	_, err = s.Solana.Chain.FundUserWithRetry(ctx, solana.DeployerPubkey, deployerFunding, 5)
	s.Require().NoError(err)

	s.T().Log("Deploying programs...")
	const keypairDir = "solana-keypairs/localnet"
	const deployerPath = keypairDir + "/deployer_wallet.json"

	deployTasks := []e2esuite.ParallelTaskWithResult[solanago.PublicKey]{
		{
			Name: "Deploy ICS26 Router",
			Run: func() (solanago.PublicKey, error) {
				keypairPath := fmt.Sprintf("%s/ics26_router-keypair.json", keypairDir)
				return s.Solana.Chain.DeploySolanaProgramAsync(ctx, "ics26_router", keypairPath, deployerPath)
			},
		},
		{
			Name: "Deploy ICS07 Tendermint",
			Run: func() (solanago.PublicKey, error) {
				keypairPath := fmt.Sprintf("%s/ics07_tendermint-keypair.json", keypairDir)
				return s.Solana.Chain.DeploySolanaProgramAsync(ctx, "ics07_tendermint", keypairPath, deployerPath)
			},
		},
		{
			Name: "Deploy Attestation Light Client",
			Run: func() (solanago.PublicKey, error) {
				keypairPath := fmt.Sprintf("%s/attestation-keypair.json", keypairDir)
				return s.Solana.Chain.DeploySolanaProgramAsync(ctx, "attestation", keypairPath, deployerPath)
			},
		},
		{
			Name: "Deploy Test App",
			Run: func() (solanago.PublicKey, error) {
				keypairPath := fmt.Sprintf("%s/test_ibc_app-keypair.json", keypairDir)
				return s.Solana.Chain.DeploySolanaProgramAsync(ctx, "test_ibc_app", keypairPath, deployerPath)
			},
		},
		{
			Name: "Deploy Access Manager",
			Run: func() (solanago.PublicKey, error) {
				keypairPath := fmt.Sprintf("%s/access_manager-keypair.json", keypairDir)
				return s.Solana.Chain.DeploySolanaProgramAsync(ctx, "access_manager", keypairPath, deployerPath)
			},
		},
	}

	deployResults, err := e2esuite.RunParallelTasksWithResults(deployTasks...)
	s.Require().NoError(err)

	s.Ics26RouterProgramID = deployResults["Deploy ICS26 Router"]
	ics26_router.ProgramID = s.Ics26RouterProgramID
	s.Ics07TendermintProgramID = deployResults["Deploy ICS07 Tendermint"]
	ics07_tendermint.ProgramID = s.Ics07TendermintProgramID
	s.AttestationLightClientProgramID = deployResults["Deploy Attestation Light Client"]
	attestation.ProgramID = s.AttestationLightClientProgramID
	s.TestAppProgramID = deployResults["Deploy Test App"]
	test_ibc_app.ProgramID = s.TestAppProgramID
	access_manager.ProgramID = deployResults["Deploy Access Manager"]

	s.T().Log("Initializing Access Manager...")
	accessControlAccount, _ := solana.AccessManager.AccessManagerPDA(access_manager.ProgramID)
	initAccessManagerInstruction, err := access_manager.NewInitializeInstruction(
		s.SolanaUser.PublicKey(),
		accessControlAccount,
		s.SolanaUser.PublicKey(),
		solanago.SystemProgramID,
		solanago.SysVarInstructionsPubkey,
	)
	s.Require().NoError(err)

	tx, err := s.Solana.Chain.NewTransactionFromInstructions(s.SolanaUser.PublicKey(), initAccessManagerInstruction)
	s.Require().NoError(err)

	_, err = s.Solana.Chain.SignAndBroadcastTxWithRetryAndTimeout(ctx, tx, rpc.CommitmentFinalized, 30, s.SolanaUser)
	s.Require().NoError(err)
	s.T().Log("Access Manager initialized")

	s.T().Log("Granting RELAYER_ROLE to SolanaUser...")
	const RELAYER_ROLE = uint64(1)
	grantRelayerRoleInstruction, err := access_manager.NewGrantRoleInstruction(
		RELAYER_ROLE,
		s.SolanaUser.PublicKey(),
		accessControlAccount,
		s.SolanaUser.PublicKey(),
		solanago.SysVarInstructionsPubkey,
	)
	s.Require().NoError(err)

	tx, err = s.Solana.Chain.NewTransactionFromInstructions(s.SolanaUser.PublicKey(), grantRelayerRoleInstruction)
	s.Require().NoError(err)

	_, err = s.Solana.Chain.SignAndBroadcastTxWithRetryAndTimeout(ctx, tx, rpc.CommitmentFinalized, 30, s.SolanaUser)
	s.Require().NoError(err)
	s.T().Log("RELAYER_ROLE granted")

	s.T().Log("Granting ID_CUSTOMIZER_ROLE to SolanaUser...")
	const ID_CUSTOMIZER_ROLE = uint64(6)
	grantIdCustomizerRoleInstruction, err := access_manager.NewGrantRoleInstruction(
		ID_CUSTOMIZER_ROLE,
		s.SolanaUser.PublicKey(),
		accessControlAccount,
		s.SolanaUser.PublicKey(),
		solanago.SysVarInstructionsPubkey,
	)
	s.Require().NoError(err)

	tx, err = s.Solana.Chain.NewTransactionFromInstructions(s.SolanaUser.PublicKey(), grantIdCustomizerRoleInstruction)
	s.Require().NoError(err)

	_, err = s.Solana.Chain.SignAndBroadcastTxWithRetryAndTimeout(ctx, tx, rpc.CommitmentFinalized, 30, s.SolanaUser)
	s.Require().NoError(err)
	s.T().Log("ID_CUSTOMIZER_ROLE granted")

	s.T().Log("Initializing ICS26 Router...")
	routerStateAccount, _ := solana.Ics26Router.RouterStatePDA(ics26_router.ProgramID)
	initInstruction, err := ics26_router.NewInitializeInstruction(
		access_manager.ProgramID,
		routerStateAccount,
		s.SolanaUser.PublicKey(),
		solanago.SystemProgramID,
	)
	s.Require().NoError(err)

	tx, err = s.Solana.Chain.NewTransactionFromInstructions(s.SolanaUser.PublicKey(), initInstruction)
	s.Require().NoError(err)

	_, err = s.Solana.Chain.SignAndBroadcastTxWithRetryAndTimeout(ctx, tx, rpc.CommitmentFinalized, 30, s.SolanaUser)
	s.Require().NoError(err)

	simd := s.Cosmos.Chains[0]
	cosmosChainID := simd.Config().ChainID

	// Set AttestationClientID to a valid IBC identifier (8-64 chars required)
	// Must use "attestations-N" format to match CLIENT_TYPE_ATTESTATION for proper PDA derivation
	s.AttestationClientID = testvalues.FirstAttestationsClientID

	// Create ALT for attestation client with attestation light client accounts
	altAddress := s.Solana.Chain.CreateIBCAddressLookupTableWithAttestation(
		ctx, s.T(), s.Require(), s.SolanaUser, cosmosChainID, transfertypes.PortID, s.AttestationClientID, s.AttestationClientID,
	)
	s.SolanaAltAddress = altAddress.String()

	s.T().Log("Starting Cosmos attestor service(s) for Cosmos → Solana direction...")
	cosmosAttestorResult := attestor.SetupAttestors(ctx, s.T(), attestor.SetupParams{
		NumAttestors:         numCosmosAttestors,
		KeystorePathTemplate: testvalues.AttestorKeystorePathTemplate,
		ChainType:            attestor.ChainTypeCosmos,
		AdapterURL:           simd.GetRPCAddress(),
		RouterAddress:        "",
		DockerClient:         s.GetDockerClient(),
		NetworkID:            s.GetNetworkID(),
	})
	s.CosmosAttestorContainers = cosmosAttestorResult.Containers
	s.CosmosAttestorEndpoints = cosmosAttestorResult.Endpoints
	s.CosmosAttestorAddresses = cosmosAttestorResult.Addresses
	s.T().Logf("Cosmos attestor addresses: %v", s.CosmosAttestorAddresses)

	s.T().Log("Starting Solana attestor service(s) for Solana → Cosmos direction...")
	solanaAttestorResult := attestor.SetupAttestors(ctx, s.T(), attestor.SetupParams{
		NumAttestors:         numSolanaAttestors,
		KeystorePathTemplate: testvalues.AttestorKeystorePathTemplate,
		ChainType:            attestor.ChainTypeSolana,
		AdapterURL:           attestor.TransformLocalhostToDockerHost(testvalues.SolanaLocalnetRPC),
		RouterAddress:        ics26_router.ProgramID.String(),
		DockerClient:         s.GetDockerClient(),
		NetworkID:            s.GetNetworkID(),
		EnableHostAccess:     true,
	})
	s.SolanaAttestorContainers = solanaAttestorResult.Containers
	s.SolanaAttestorEndpoints = solanaAttestorResult.Endpoints
	s.SolanaAttestorAddresses = solanaAttestorResult.Addresses
	if len(s.SolanaAttestorEndpoints) > 0 {
		serverAddr := s.SolanaAttestorEndpoints[0][len("http://"):]
		s.SolanaAttestorClient, err = attestor.GetAttestationServiceClient(serverAddr)
		s.Require().NoError(err)
	}
	s.T().Logf("Solana attestor endpoints: %v", s.SolanaAttestorEndpoints)
	s.T().Logf("Solana attestor addresses: %v", s.SolanaAttestorAddresses)

	s.T().Log("Initializing Attestation Light Client on Solana...")
	s.initializeAttestationLightClient(ctx)

	s.T().Log("Starting relayer...")
	config := relayer.NewConfigBuilder().
		SolanaToCosmosAttested(relayer.SolanaToCosmosAttestedParams{
			SolanaChainID:     testvalues.SolanaChainID,
			CosmosChainID:     simd.Config().ChainID,
			SolanaRPC:         testvalues.SolanaLocalnetRPC,
			TmRPC:             simd.GetHostRPCAddress(),
			ICS26ProgramID:    ics26_router.ProgramID.String(),
			SignerAddress:     s.Cosmos.Users[0].FormattedAddress(),
			AttestorEndpoints: s.SolanaAttestorEndpoints,
			AttestorTimeout:   30000,
			QuorumThreshold:   testvalues.DefaultMinRequiredSigs,
		}).
		CosmosToSolanaAttested(relayer.CosmosToSolanaAttestedParams{
			CosmosChainID:     simd.Config().ChainID,
			SolanaChainID:     testvalues.SolanaChainID,
			SolanaRPC:         testvalues.SolanaLocalnetRPC,
			TmRPC:             simd.GetHostRPCAddress(),
			ICS26ProgramID:    ics26_router.ProgramID.String(),
			FeePayer:          s.SolanaUser.PublicKey().String(),
			ALTAddress:        s.SolanaAltAddress,
			AttestorEndpoints: s.CosmosAttestorEndpoints,
		}).
		Build()

	err = config.GenerateConfigFile(testvalues.RelayerConfigFilePath)
	s.Require().NoError(err)

	s.RelayerProcess, err = relayer.StartRelayer(testvalues.RelayerConfigFilePath)
	s.Require().NoError(err)

	s.T().Cleanup(func() {
		os.Remove(testvalues.RelayerConfigFilePath)
	})

	s.RelayerClient, err = relayer.GetGRPCClient(relayer.DefaultRelayerGRPCAddress())
	s.Require().NoError(err)

	s.T().Log("Creating attestor client on Cosmos...")
	currentFinalizedSlot, err := s.Solana.Chain.RPCClient.GetSlot(ctx, rpc.CommitmentFinalized)
	s.Require().NoError(err)
	solanaTimestamp, err := s.Solana.Chain.RPCClient.GetBlockTime(ctx, currentFinalizedSlot)
	s.Require().NoError(err)

	s.Require().NotEmpty(s.SolanaAttestorAddresses, "No Solana attestor addresses available")

	clientResp, err := s.RelayerClient.CreateClient(ctx, &relayertypes.CreateClientRequest{
		SrcChain: testvalues.SolanaChainID,
		DstChain: simd.Config().ChainID,
		Parameters: map[string]string{
			testvalues.ParameterKey_AttestorAddresses: strings.Join(s.SolanaAttestorAddresses, ","),
			testvalues.ParameterKey_MinRequiredSigs:   strconv.Itoa(numSolanaAttestors),
			testvalues.ParameterKey_height:            strconv.FormatUint(currentFinalizedSlot, 10),
			testvalues.ParameterKey_timestamp:         strconv.FormatInt(int64(*solanaTimestamp), 10),
		},
	})
	s.Require().NoError(err)

	txResp := s.MustBroadcastSdkTxBody(ctx, simd, s.Cosmos.Users[0], CosmosCreateClientGasLimit, clientResp.Tx)
	s.T().Logf("Attestor client created on Cosmos - tx: %s", txResp.TxHash)

	s.T().Log("Adding attestation client to Router on Solana...")
	routerStateAccount, _ = solana.Ics26Router.RouterStatePDA(ics26_router.ProgramID)
	clientAccount, _ := solana.Ics26Router.ClientWithArgSeedPDA(ics26_router.ProgramID, []byte(s.AttestationClientID))
	clientSequenceAccount, _ := solana.Ics26Router.ClientSequenceWithArgSeedPDA(ics26_router.ProgramID, []byte(s.AttestationClientID))

	counterpartyInfo := ics26_router.SolanaIbcTypesRouterCounterpartyInfo{
		ClientId:     CosmosClientID,
		MerklePrefix: [][]byte{[]byte("")}, // Single element for attestation light client compatibility
	}

	addClientInstruction, err := ics26_router.NewAddClientInstruction(
		s.AttestationClientID,
		counterpartyInfo,
		s.SolanaUser.PublicKey(),
		routerStateAccount,
		accessControlAccount,
		clientAccount,
		clientSequenceAccount,
		attestation.ProgramID,
		solanago.SystemProgramID,
		solanago.SysVarInstructionsPubkey,
	)
	s.Require().NoError(err)

	tx, err = s.Solana.Chain.NewTransactionFromInstructions(s.SolanaUser.PublicKey(), addClientInstruction)
	s.Require().NoError(err)

	_, err = s.Solana.Chain.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentFinalized, s.SolanaUser)
	s.Require().NoError(err)
	s.T().Log("Attestation client added to router")

	s.T().Log("Registering counterparty on Cosmos...")
	merklePathPrefix := [][]byte{[]byte("")}
	_, err = s.BroadcastMessages(ctx, simd, s.Cosmos.Users[0], CosmosDefaultGasLimit, &clienttypesv2.MsgRegisterCounterparty{
		ClientId:                 CosmosClientID,
		CounterpartyMerklePrefix: merklePathPrefix,
		CounterpartyClientId:     s.AttestationClientID,
		Signer:                   s.Cosmos.Users[0].FormattedAddress(),
	})
	s.Require().NoError(err)
	s.T().Log("Counterparty registered on Cosmos")

	s.T().Log("Initializing Test IBC App...")
	appStateAccount, _ := solana.TestIbcApp.AppStateTransferPDA(s.TestAppProgramID)

	initAppInstruction, err := test_ibc_app.NewInitializeInstruction(
		s.SolanaUser.PublicKey(),
		appStateAccount,
		s.SolanaUser.PublicKey(),
		solanago.SystemProgramID,
	)
	s.Require().NoError(err)

	appTx, err := s.Solana.Chain.NewTransactionFromInstructions(s.SolanaUser.PublicKey(), initAppInstruction)
	s.Require().NoError(err)

	_, err = s.Solana.Chain.SignAndBroadcastTxWithRetry(ctx, appTx, rpc.CommitmentFinalized, s.SolanaUser)
	s.Require().NoError(err)
	s.T().Log("Test IBC App initialized successfully")

	s.T().Log("Registering Test App with Router...")
	routerStateAccount, _ = solana.Ics26Router.RouterStatePDA(ics26_router.ProgramID)
	ibcAppAccount, _ := solana.Ics26Router.IbcAppWithArgSeedPDA(ics26_router.ProgramID, []byte(transfertypes.PortID))

	registerInstruction, err := ics26_router.NewAddIbcAppInstruction(
		transfertypes.PortID,
		routerStateAccount,
		accessControlAccount,
		ibcAppAccount,
		s.TestAppProgramID,
		s.SolanaUser.PublicKey(),
		s.SolanaUser.PublicKey(),
		solanago.SystemProgramID,
		solanago.SysVarInstructionsPubkey,
	)
	s.Require().NoError(err)

	tx, err = s.Solana.Chain.NewTransactionFromInstructions(s.SolanaUser.PublicKey(), registerInstruction)
	s.Require().NoError(err)

	_, err = s.Solana.Chain.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentFinalized, s.SolanaUser)
	s.Require().NoError(err)
	s.T().Log("Test app registered with router")
}

// initializeAttestationLightClient initializes the attestation light client with Cosmos attestor addresses
func (s *IbcSolanaAttestationTestSuite) initializeAttestationLightClient(ctx context.Context) {
	var attestorAddresses [][20]uint8
	for _, addr := range s.CosmosAttestorAddresses {
		addrHex := addr
		if len(addrHex) >= 2 && addrHex[:2] == "0x" {
			addrHex = addrHex[2:]
		}

		addrBytes, err := hex.DecodeString(addrHex)
		s.Require().NoError(err)
		s.Require().Len(addrBytes, 20, "Attestor address must be 20 bytes")

		var addrArray [20]uint8
		copy(addrArray[:], addrBytes)
		attestorAddresses = append(attestorAddresses, addrArray)
	}

	minRequiredSigs := uint8(numCosmosAttestors)

	clientStatePDA, _ := solana.Attestation.ClientPDA(attestation.ProgramID)
	appStatePDA, _ := solana.Attestation.AppStatePDA(attestation.ProgramID)

	initInstruction, err := attestation.NewInitializeInstruction(
		attestorAddresses,
		minRequiredSigs,
		access_manager.ProgramID,
		clientStatePDA,
		appStatePDA,
		s.SolanaUser.PublicKey(),
		solanago.SystemProgramID,
	)
	s.Require().NoError(err)

	tx, err := s.Solana.Chain.NewTransactionFromInstructions(s.SolanaUser.PublicKey(), initInstruction)
	s.Require().NoError(err)

	sig, err := s.Solana.Chain.SignAndBroadcastTxWithRetryAndTimeout(ctx, tx, rpc.CommitmentFinalized, 30, s.SolanaUser)
	s.Require().NoError(err)
	s.T().Logf("Attestation Light Client initialized - tx: %s", sig)
}

func (s *IbcSolanaAttestationTestSuite) Test_Attestation_Deploy() {
	ctx := context.Background()
	s.SetupSuite(ctx)

	s.Require().True(s.Run("Verify attestation light client on Solana", func() {
		clientStatePDA, _ := solana.Attestation.ClientPDA(attestation.ProgramID)

		accountInfo, err := s.Solana.Chain.RPCClient.GetAccountInfoWithOpts(ctx, clientStatePDA, &rpc.GetAccountInfoOpts{
			Commitment: rpc.CommitmentFinalized,
		})
		s.Require().NoError(err)

		clientState, err := attestation.ParseAccount_AttestationTypesClientState(accountInfo.Value.Data.GetBinary())
		s.Require().NoError(err)

		s.Require().Equal(uint8(numCosmosAttestors), clientState.MinRequiredSigs)
		s.Require().Equal(uint64(0), clientState.LatestHeight)
		s.Require().False(clientState.IsFrozen)
		s.Require().Len(clientState.AttestorAddresses, len(s.CosmosAttestorAddresses))

		s.T().Logf("Attestation LC: latestHeight=%d, attestors=%d",
			clientState.LatestHeight, len(clientState.AttestorAddresses))
	}))

	s.Require().True(s.Run("Verify Solana attestor is running", func() {
		slot, err := s.Solana.Chain.RPCClient.GetSlot(ctx, rpc.CommitmentFinalized)
		s.Require().NoError(err)

		resp, err := attestor.GetStateAttestation(ctx, s.SolanaAttestorClient, slot)
		s.Require().NoError(err)
		s.Require().NotNil(resp.GetAttestation())

		attestation := resp.GetAttestation()
		s.Require().NotEmpty(attestation.GetSignature())
		s.Require().Equal(slot, attestation.GetHeight())
		s.T().Logf("Solana attestor working - attested slot: %d", slot)
	}))

	s.Require().True(s.Run("Verify relayer connection", func() {
		simd := s.Cosmos.Chains[0]

		resp, err := s.RelayerClient.Info(ctx, &relayertypes.InfoRequest{
			SrcChain: simd.Config().ChainID,
			DstChain: testvalues.SolanaChainID,
		})
		s.Require().NoError(err)
		s.Require().NotNil(resp)
		s.T().Logf("Relayer: %s <-> %s", resp.SourceChain.ChainId, resp.TargetChain.ChainId)
	}))
}

func (s *IbcSolanaAttestationTestSuite) Test_Attestation_SolanaAttestorVerifyPacketCommitment() {
	ctx := context.Background()
	s.SetupSuite(ctx)

	var solanaTxSig solanago.Signature
	var baseSequence uint64
	var namespacedSequence uint64
	var packetCommitmentPDA solanago.PublicKey
	var slot uint64

	s.Require().True(s.Run("Send packet on Solana", func() {
		appState, _ := solana.TestIbcApp.AppStateTransferPDA(s.TestAppProgramID)
		routerState, _ := solana.Ics26Router.RouterStatePDA(ics26_router.ProgramID)
		ibcApp, _ := solana.Ics26Router.IbcAppWithArgSeedPDA(ics26_router.ProgramID, []byte(transfertypes.PortID))
		client, _ := solana.Ics26Router.ClientWithArgSeedPDA(ics26_router.ProgramID, []byte(s.AttestationClientID))
		clientSequence, _ := solana.Ics26Router.ClientSequenceWithArgSeedPDA(ics26_router.ProgramID, []byte(s.AttestationClientID))

		clientSequenceAccountInfo, err := s.Solana.Chain.RPCClient.GetAccountInfoWithOpts(ctx, clientSequence, &rpc.GetAccountInfoOpts{
			Commitment: rpc.CommitmentFinalized,
		})
		s.Require().NoError(err)

		clientSequenceData, err := ics26_router.ParseAccount_Ics26RouterStateClientSequence(clientSequenceAccountInfo.Value.Data.GetBinary())
		s.Require().NoError(err)
		baseSequence = clientSequenceData.NextSequenceSend

		namespacedSequence = solana.CalculateNamespacedSequence(baseSequence, s.TestAppProgramID, s.SolanaUser.PublicKey())

		namespacedSequenceBytes := make([]byte, 8)
		binary.LittleEndian.PutUint64(namespacedSequenceBytes, namespacedSequence)
		packetCommitmentPDA, _ = solana.Ics26Router.PacketCommitmentWithArgSeedPDA(ics26_router.ProgramID, []byte(s.AttestationClientID), namespacedSequenceBytes)

		timeoutTimestamp := time.Now().Add(1 * time.Hour).Unix()
		packetMsg := test_ibc_app.TestIbcAppInstructionsSendPacketSendPacketMsg{
			SourceClient:     s.AttestationClientID,
			SourcePort:       transfertypes.PortID,
			DestPort:         transfertypes.PortID,
			Version:          transfertypes.V1,
			Encoding:         "application/json",
			PacketData:       []byte(`{"test":"data"}`),
			TimeoutTimestamp: timeoutTimestamp,
		}

		attestationClientStatePDA, _ := solana.Attestation.ClientPDA(attestation.ProgramID)
		attestationConsensusStatePDA := s.deriveAttestationConsensusStatePDA(ctx, attestationClientStatePDA)
		sendPacketInstruction, err := test_ibc_app.NewSendPacketInstruction(
			packetMsg,
			appState,
			s.SolanaUser.PublicKey(),
			routerState,
			ibcApp,
			clientSequence,
			packetCommitmentPDA,
			client,
			attestation.ProgramID,
			attestationClientStatePDA,
			attestationConsensusStatePDA,
			ics26_router.ProgramID,
			solanago.SystemProgramID,
		)
		s.Require().NoError(err)

		computeBudgetInstruction := solana.NewComputeBudgetInstruction(DefaultComputeUnits)
		tx, err := s.Solana.Chain.NewTransactionFromInstructions(
			s.SolanaUser.PublicKey(),
			computeBudgetInstruction,
			sendPacketInstruction,
		)
		s.Require().NoError(err)

		solanaTxSig, err = s.Solana.Chain.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentFinalized, s.SolanaUser)
		s.Require().NoError(err)

		slot, err = s.Solana.Chain.RPCClient.GetSlot(ctx, rpc.CommitmentFinalized)
		s.Require().NoError(err)

		s.T().Logf("Sent packet - tx: %s, sequence: %d, slot: %d", solanaTxSig, namespacedSequence, slot)
	}))

	var event *solana.SendPacketEvent
	s.Require().True(s.Run("Parse event and verify sequence", func() {
		var err error
		event, err = solana.GetSendPacketEventFromTransaction(ctx, s.Solana.Chain.RPCClient, solanaTxSig)
		s.Require().NoError(err)
		s.Require().NotNil(event)
		s.Require().Equal(s.AttestationClientID, event.ClientID)
		s.Require().Equal(namespacedSequence, event.Sequence)
	}))

	var onChainCommitment []byte
	s.Require().True(s.Run("Query commitment on chain", func() {
		accountInfo, err := s.Solana.Chain.RPCClient.GetAccountInfoWithOpts(ctx, packetCommitmentPDA, &rpc.GetAccountInfoOpts{
			Commitment: rpc.CommitmentFinalized,
		})
		s.Require().NoError(err)
		s.Require().NotNil(accountInfo.Value)

		data := accountInfo.Value.Data.GetBinary()
		s.Require().GreaterOrEqual(len(data), 40)

		onChainCommitment = data[8:40]
		s.Require().NotEmpty(onChainCommitment)
	}))

	s.Require().True(s.Run("Call Solana attestor and verify commitment", func() {
		abiPacket := convertSolanaPacketToABI(event.Packet)
		packetBytes, err := types.AbiEncodePacket(abiPacket)
		s.Require().NoError(err)

		resp, err := attestor.GetPacketAttestation(ctx, s.SolanaAttestorClient, [][]byte{packetBytes}, slot)
		s.Require().NoError(err)
		s.Require().NotNil(resp.GetAttestation())

		attestation := resp.GetAttestation()
		s.Require().NotEmpty(attestation.GetSignature())
		s.Require().Equal(slot, attestation.GetHeight())

		attestedData := attestation.GetAttestedData()

		packetAttestation, err := types.AbiDecodePacketAttestation(attestedData)
		s.Require().NoError(err)
		s.Require().NotNil(packetAttestation)
		s.Require().Equal(slot, packetAttestation.Height)
		s.Require().Len(packetAttestation.Packets, 1)

		attestorCommitment := packetAttestation.Packets[0].Commitment[:]
		s.Require().Equal(onChainCommitment, attestorCommitment)

		s.T().Log("Solana attestor packet commitment verification successful")
	}))
}

func (s *IbcSolanaAttestationTestSuite) Test_Attestation_CosmosToSolanaTransfer() {
	ctx := context.Background()
	s.SetupSuite(ctx)

	simd := s.Cosmos.Chains[0]

	var cosmosRelayPacketTxHash []byte
	var timeout uint64
	var encodedPayload []byte

	s.Require().True(s.Run("Send ICS20 transfer from Cosmos to Solana", func() {
		cosmosUserWallet := s.Cosmos.Users[0]
		cosmosUserAddress := cosmosUserWallet.FormattedAddress()
		solanaUserAddress := s.SolanaUser.PublicKey().String()
		transferCoin := sdk.NewCoin(simd.Config().Denom, sdkmath.NewInt(TestTransferAmount))

		timeout = uint64(time.Now().Add(30 * time.Minute).Unix())

		transferPayload := transfertypes.FungibleTokenPacketData{
			Denom:    transferCoin.Denom,
			Amount:   transferCoin.Amount.String(),
			Sender:   cosmosUserAddress,
			Receiver: solanaUserAddress,
			Memo:     "cosmos-to-solana-attestation-transfer",
		}
		var err error
		encodedPayload, err = transfertypes.MarshalPacketData(transferPayload, transfertypes.V1, transfertypes.EncodingProtobuf)
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

		resp, err := s.BroadcastMessages(ctx, simd, cosmosUserWallet, CosmosDefaultGasLimit, &msgSendPacket)
		s.Require().NoError(err)
		s.Require().NotEmpty(resp.TxHash)

		cosmosPacketTxHashBytes, err := hex.DecodeString(resp.TxHash)
		s.Require().NoError(err)
		cosmosRelayPacketTxHash = cosmosPacketTxHashBytes

		s.T().Logf("Cosmos packet sent: %s", resp.TxHash)
	}))

	s.Require().NoError(testutil.WaitForBlocks(ctx, 1, simd))

	var solanaRelayTxSig solanago.Signature

	s.Require().True(s.Run("Relay packet to Solana using attestation LC", func() {
		s.Require().True(s.Run("Update attestation client on Solana", func() {
			resp, err := s.RelayerClient.UpdateClient(ctx, &relayertypes.UpdateClientRequest{
				SrcChain:    simd.Config().ChainID,
				DstChain:    testvalues.SolanaChainID,
				DstClientId: s.AttestationClientID,
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Tx)

			// Unmarshal the protobuf-encoded SolanaUpdateClient
			var solanaUpdateClient relayertypes.SolanaUpdateClient
			err = proto.Unmarshal(resp.Tx, &solanaUpdateClient)
			s.Require().NoError(err, "Failed to unmarshal SolanaUpdateClient")
			s.Require().NotEmpty(solanaUpdateClient.AssemblyTx, "AssemblyTx is empty")

			unsignedSolanaTx, err := solanago.TransactionFromDecoder(bin.NewBinDecoder(solanaUpdateClient.AssemblyTx))
			s.Require().NoError(err)

			sig, err := s.Solana.Chain.SignAndBroadcastTxWithRetry(ctx, unsignedSolanaTx, rpc.CommitmentFinalized, s.SolanaUser)
			s.Require().NoError(err)
			s.T().Logf("Attestation client updated - tx: %s", sig)
		}))

		s.Require().True(s.Run("Relay packet", func() {
			resp, err := s.RelayerClient.RelayByTx(ctx, &relayertypes.RelayByTxRequest{
				SrcChain:    simd.Config().ChainID,
				DstChain:    testvalues.SolanaChainID,
				SourceTxIds: [][]byte{cosmosRelayPacketTxHash},
				SrcClientId: CosmosClientID,
				DstClientId: s.AttestationClientID,
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Tx)

			var err2 error
			solanaRelayTxSig, err2 = s.Solana.Chain.SubmitChunkedRelayPackets(ctx, s.T(), resp, s.SolanaUser)
			s.Require().NoError(err2)
			s.T().Logf("Packet relayed to Solana - tx: %s", solanaRelayTxSig)
		}))
	}))

	s.Require().True(s.Run("Verify packet received on Solana", func() {
		testAppStateAccount, _ := solana.TestIbcApp.AppStateTransferPDA(s.TestAppProgramID)

		accountInfo, err := s.Solana.Chain.RPCClient.GetAccountInfoWithOpts(ctx, testAppStateAccount, &rpc.GetAccountInfoOpts{
			Commitment: rpc.CommitmentConfirmed,
		})
		s.Require().NoError(err)
		s.Require().NotNil(accountInfo.Value)

		appState, err := test_ibc_app.ParseAccount_TestIbcAppStateTestIbcAppState(accountInfo.Value.Data.GetBinary())
		s.Require().NoError(err)

		s.Require().Greater(appState.PacketsReceived, uint64(0))
		s.T().Logf("Solana dummy app received %d packets", appState.PacketsReceived)
	}))

	s.Require().True(s.Run("Verify ACK commitment via Solana attestor", func() {
		s.Require().NoError(s.Solana.Chain.WaitForTxStatus(solanaRelayTxSig, rpc.ConfirmationStatusFinalized))

		slot, err := s.Solana.Chain.RPCClient.GetSlot(ctx, rpc.CommitmentFinalized)
		s.Require().NoError(err)

		actualSequence := uint64(1)
		sequenceBytes := make([]byte, 8)
		binary.LittleEndian.PutUint64(sequenceBytes, actualSequence)

		packetAckPDA, _ := solana.Ics26Router.PacketAckWithArgSeedPDA(ics26_router.ProgramID, []byte(s.AttestationClientID), sequenceBytes)

		accountInfo, err := s.Solana.Chain.RPCClient.GetAccountInfoWithOpts(ctx, packetAckPDA, &rpc.GetAccountInfoOpts{
			Commitment: rpc.CommitmentFinalized,
		})
		s.Require().NoError(err)
		s.Require().NotNil(accountInfo.Value)

		data := accountInfo.Value.Data.GetBinary()
		s.Require().GreaterOrEqual(len(data), 40)
		onChainAckCommitment := data[8:40]
		s.Require().NotEmpty(onChainAckCommitment)

		abiPacket := ics26router.IICS26RouterMsgsPacket{
			Sequence:         1,
			SourceClient:     CosmosClientID,
			DestClient:       s.AttestationClientID,
			TimeoutTimestamp: timeout,
			Payloads: []ics26router.IICS26RouterMsgsPayload{
				{
					SourcePort: transfertypes.PortID,
					DestPort:   transfertypes.PortID,
					Version:    transfertypes.V1,
					Encoding:   transfertypes.EncodingProtobuf,
					Value:      encodedPayload,
				},
			},
		}
		packetBytes, err := types.AbiEncodePacket(abiPacket)
		s.Require().NoError(err)

		resp, err := attestor.GetPacketAttestation(ctx, s.SolanaAttestorClient, [][]byte{packetBytes}, slot, attestortypes.CommitmentType_COMMITMENT_TYPE_ACK)
		s.Require().NoError(err)
		s.Require().NotNil(resp.GetAttestation())

		attestation := resp.GetAttestation()
		s.Require().NotEmpty(attestation.GetSignature())
		s.Require().Equal(slot, attestation.GetHeight())

		attestedData := attestation.GetAttestedData()
		packetAttestation, err := types.AbiDecodePacketAttestation(attestedData)
		s.Require().NoError(err)
		s.Require().NotNil(packetAttestation)
		s.Require().Len(packetAttestation.Packets, 1)

		attestorAckCommitment := packetAttestation.Packets[0].Commitment[:]
		s.Require().Equal(onChainAckCommitment, attestorAckCommitment)

		s.T().Log("Solana attestor ACK commitment verification successful")
	}))

	s.Require().True(s.Run("Relay acknowledgment back to Cosmos", func() {
		resp, err := s.RelayerClient.RelayByTx(ctx, &relayertypes.RelayByTxRequest{
			SrcChain:    testvalues.SolanaChainID,
			DstChain:    simd.Config().ChainID,
			SourceTxIds: [][]byte{[]byte(solanaRelayTxSig.String())},
			SrcClientId: s.AttestationClientID,
			DstClientId: CosmosClientID,
		})
		s.Require().NoError(err)
		s.Require().NotEmpty(resp.Tx)

		relayTxResult := s.MustBroadcastSdkTxBody(ctx, simd, s.Cosmos.Users[0], CosmosDefaultGasLimit, resp.Tx)
		s.T().Logf("ACK relayed to Cosmos - tx: %s", relayTxResult.TxHash)
	}))

	s.Require().True(s.Run("Verify balance on Cosmos", func() {
		cosmosUserAddress := s.Cosmos.Users[0].FormattedAddress()

		allBalancesResp, err := e2esuite.GRPCQuery[banktypes.QueryAllBalancesResponse](ctx, simd, &banktypes.QueryAllBalancesRequest{
			Address: cosmosUserAddress,
		})
		s.Require().NoError(err)
		s.T().Logf("Cosmos user balances:")
		for _, balance := range allBalancesResp.Balances {
			s.T().Logf("  - %s: %s", balance.Denom, balance.Amount.String())
		}
	}))
}

func (s *IbcSolanaAttestationTestSuite) Test_Attestation_Roundtrip() {
	ctx := context.Background()
	s.SetupSuite(ctx)

	simd := s.Cosmos.Chains[0]
	cosmosUserWallet := s.Cosmos.Users[0]
	cosmosUserAddress := cosmosUserWallet.FormattedAddress()
	solanaUserAddress := s.SolanaUser.PublicKey().String()
	transferCoin := sdk.NewCoin(simd.Config().Denom, sdkmath.NewInt(TestTransferAmount))

	var initialCosmosBalance int64
	var cosmosRelayPacketTxHash []byte
	var solanaRelayTxSig solanago.Signature
	var cosmosPacketSequence uint64

	s.Require().True(s.Run("Phase 1: Cosmos to Solana transfer", func() {
		s.Require().True(s.Run("Record initial Cosmos balance", func() {
			resp, err := e2esuite.GRPCQuery[banktypes.QueryBalanceResponse](ctx, simd, &banktypes.QueryBalanceRequest{
				Address: cosmosUserAddress,
				Denom:   transferCoin.Denom,
			})
			s.Require().NoError(err)
			s.Require().NotNil(resp.Balance)
			initialCosmosBalance = resp.Balance.Amount.Int64()
			s.T().Logf("Initial Cosmos balance: %d %s", initialCosmosBalance, transferCoin.Denom)
		}))

		s.Require().True(s.Run("Send ICS20 transfer from Cosmos", func() {
			timeout := uint64(time.Now().Add(30 * time.Minute).Unix())

			transferPayload := transfertypes.FungibleTokenPacketData{
				Denom:    transferCoin.Denom,
				Amount:   transferCoin.Amount.String(),
				Sender:   cosmosUserAddress,
				Receiver: solanaUserAddress,
				Memo:     "roundtrip-cosmos-to-solana",
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
				Payloads:         []channeltypesv2.Payload{payload},
				Signer:           cosmosUserAddress,
			}

			resp, err := s.BroadcastMessages(ctx, simd, cosmosUserWallet, CosmosDefaultGasLimit, &msgSendPacket)
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.TxHash)

			cosmosPacketTxHashBytes, err := hex.DecodeString(resp.TxHash)
			s.Require().NoError(err)
			cosmosRelayPacketTxHash = cosmosPacketTxHashBytes
			cosmosPacketSequence = 1

			s.T().Logf("Cosmos → Solana packet sent: %s (sequence: %d)", resp.TxHash, cosmosPacketSequence)
		}))

		s.Require().True(s.Run("Verify packet commitment exists on Cosmos", func() {
			commitmentResp, err := e2esuite.GRPCQuery[channeltypesv2.QueryPacketCommitmentResponse](ctx, simd, &channeltypesv2.QueryPacketCommitmentRequest{
				ClientId: CosmosClientID,
				Sequence: cosmosPacketSequence,
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(commitmentResp.Commitment)
			s.T().Logf("Cosmos packet commitment verified for sequence %d", cosmosPacketSequence)
		}))
	}))

	s.Require().NoError(testutil.WaitForBlocks(ctx, 1, simd))

	var initialPacketsReceived uint64

	s.Require().True(s.Run("Relay packet to Solana via attestation LC", func() {
		s.Require().True(s.Run("Record initial packets received on Solana", func() {
			testAppStateAccount, _ := solana.TestIbcApp.AppStateTransferPDA(s.TestAppProgramID)
			accountInfo, err := s.Solana.Chain.RPCClient.GetAccountInfoWithOpts(ctx, testAppStateAccount, &rpc.GetAccountInfoOpts{
				Commitment: rpc.CommitmentConfirmed,
			})
			s.Require().NoError(err)
			if accountInfo.Value != nil {
				appState, err := test_ibc_app.ParseAccount_TestIbcAppStateTestIbcAppState(accountInfo.Value.Data.GetBinary())
				s.Require().NoError(err)
				initialPacketsReceived = appState.PacketsReceived
			}
			s.T().Logf("Initial packets received on Solana: %d", initialPacketsReceived)
		}))

		s.Require().True(s.Run("Update attestation client on Solana", func() {
			resp, err := s.RelayerClient.UpdateClient(ctx, &relayertypes.UpdateClientRequest{
				SrcChain:    simd.Config().ChainID,
				DstChain:    testvalues.SolanaChainID,
				DstClientId: s.AttestationClientID,
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Tx)

			// Unmarshal the protobuf-encoded SolanaUpdateClient
			var solanaUpdateClient relayertypes.SolanaUpdateClient
			err = proto.Unmarshal(resp.Tx, &solanaUpdateClient)
			s.Require().NoError(err, "Failed to unmarshal SolanaUpdateClient")
			s.Require().NotEmpty(solanaUpdateClient.AssemblyTx, "AssemblyTx is empty")

			unsignedSolanaTx, err := solanago.TransactionFromDecoder(bin.NewBinDecoder(solanaUpdateClient.AssemblyTx))
			s.Require().NoError(err)

			sig, err := s.Solana.Chain.SignAndBroadcastTxWithRetry(ctx, unsignedSolanaTx, rpc.CommitmentFinalized, s.SolanaUser)
			s.Require().NoError(err)
			s.T().Logf("Attestation client updated - tx: %s", sig)
		}))

		s.Require().True(s.Run("Relay packet to Solana", func() {
			resp, err := s.RelayerClient.RelayByTx(ctx, &relayertypes.RelayByTxRequest{
				SrcChain:    simd.Config().ChainID,
				DstChain:    testvalues.SolanaChainID,
				SourceTxIds: [][]byte{cosmosRelayPacketTxHash},
				SrcClientId: CosmosClientID,
				DstClientId: s.AttestationClientID,
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Tx)

			var err2 error
			solanaRelayTxSig, err2 = s.Solana.Chain.SubmitChunkedRelayPackets(ctx, s.T(), resp, s.SolanaUser)
			s.Require().NoError(err2)
			s.T().Logf("Packet relayed to Solana - tx: %s", solanaRelayTxSig)
		}))

		s.Require().True(s.Run("Verify packet received on Solana", func() {
			testAppStateAccount, _ := solana.TestIbcApp.AppStateTransferPDA(s.TestAppProgramID)
			accountInfo, err := s.Solana.Chain.RPCClient.GetAccountInfoWithOpts(ctx, testAppStateAccount, &rpc.GetAccountInfoOpts{
				Commitment: rpc.CommitmentConfirmed,
			})
			s.Require().NoError(err)
			s.Require().NotNil(accountInfo.Value)

			appState, err := test_ibc_app.ParseAccount_TestIbcAppStateTestIbcAppState(accountInfo.Value.Data.GetBinary())
			s.Require().NoError(err)

			s.Require().Greater(appState.PacketsReceived, initialPacketsReceived)
			s.T().Logf("Solana dummy app received %d packets (was %d)", appState.PacketsReceived, initialPacketsReceived)
		}))

		s.Require().True(s.Run("Relay ACK to Cosmos", func() {
			resp, err := s.RelayerClient.RelayByTx(ctx, &relayertypes.RelayByTxRequest{
				SrcChain:    testvalues.SolanaChainID,
				DstChain:    simd.Config().ChainID,
				SourceTxIds: [][]byte{[]byte(solanaRelayTxSig.String())},
				SrcClientId: s.AttestationClientID,
				DstClientId: CosmosClientID,
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Tx)

			relayTxResult := s.MustBroadcastSdkTxBody(ctx, simd, cosmosUserWallet, CosmosDefaultGasLimit, resp.Tx)
			s.T().Logf("ACK relayed to Cosmos - tx: %s", relayTxResult.TxHash)
		}))

		s.Require().True(s.Run("Verify packet commitment deleted on Cosmos", func() {
			_, err := e2esuite.GRPCQuery[channeltypesv2.QueryPacketCommitmentResponse](ctx, simd, &channeltypesv2.QueryPacketCommitmentRequest{
				ClientId: CosmosClientID,
				Sequence: cosmosPacketSequence,
			})
			s.Require().ErrorContains(err, "packet commitment hash not found")
			s.T().Logf("Cosmos packet commitment deleted for sequence %d", cosmosPacketSequence)
		}))
	}))

	var solanaSendTxSig solanago.Signature
	var sendPacketSlot uint64
	var sendPacketEvent *solana.SendPacketEvent
	var solanaBaseSequence uint64

	s.Require().True(s.Run("Phase 2: Solana to Cosmos transfer", func() {
		routerState, _ := solana.Ics26Router.RouterStatePDA(ics26_router.ProgramID)
		ibcApp, _ := solana.Ics26Router.IbcAppWithArgSeedPDA(ics26_router.ProgramID, []byte(transfertypes.PortID))
		client, _ := solana.Ics26Router.ClientWithArgSeedPDA(ics26_router.ProgramID, []byte(s.AttestationClientID))
		clientSequence, _ := solana.Ics26Router.ClientSequenceWithArgSeedPDA(ics26_router.ProgramID, []byte(s.AttestationClientID))
		appState, _ := solana.TestIbcApp.AppStateTransferPDA(s.TestAppProgramID)

		s.Require().True(s.Run("Send packet from Solana", func() {
			clientSequenceAccountInfo, err := s.Solana.Chain.RPCClient.GetAccountInfoWithOpts(ctx, clientSequence, &rpc.GetAccountInfoOpts{
				Commitment: rpc.CommitmentFinalized,
			})
			s.Require().NoError(err)

			clientSequenceData, err := ics26_router.ParseAccount_Ics26RouterStateClientSequence(clientSequenceAccountInfo.Value.Data.GetBinary())
			s.Require().NoError(err)
			solanaBaseSequence = clientSequenceData.NextSequenceSend

			namespacedSequence := solana.CalculateNamespacedSequence(solanaBaseSequence, s.TestAppProgramID, s.SolanaUser.PublicKey())
			namespacedSequenceBytes := make([]byte, 8)
			binary.LittleEndian.PutUint64(namespacedSequenceBytes, namespacedSequence)
			packetCommitmentPDA, _ := solana.Ics26Router.PacketCommitmentWithArgSeedPDA(ics26_router.ProgramID, []byte(s.AttestationClientID), namespacedSequenceBytes)

			timeoutTimestamp := time.Now().Add(1 * time.Hour).Unix()
			packetMsg := test_ibc_app.TestIbcAppInstructionsSendPacketSendPacketMsg{
				SourceClient:     s.AttestationClientID,
				SourcePort:       transfertypes.PortID,
				DestPort:         transfertypes.PortID,
				Version:          transfertypes.V1,
				Encoding:         "application/json",
				PacketData:       []byte(fmt.Sprintf(`{"denom":"%s","amount":"%d","sender":"%s","receiver":"%s","memo":"roundtrip-solana-to-cosmos"}`, transferCoin.Denom, TestTransferAmount, solanaUserAddress, cosmosUserAddress)),
				TimeoutTimestamp: timeoutTimestamp,
			}

			attestationClientStatePDA, _ := solana.Attestation.ClientPDA(attestation.ProgramID)
			attestationConsensusStatePDA := s.deriveAttestationConsensusStatePDA(ctx, attestationClientStatePDA)
			sendPacketInstruction, err := test_ibc_app.NewSendPacketInstruction(
				packetMsg,
				appState,
				s.SolanaUser.PublicKey(),
				routerState,
				ibcApp,
				clientSequence,
				packetCommitmentPDA,
				client,
				attestation.ProgramID,
				attestationClientStatePDA,
				attestationConsensusStatePDA,
				ics26_router.ProgramID,
				solanago.SystemProgramID,
			)
			s.Require().NoError(err)

			computeBudgetInstruction := solana.NewComputeBudgetInstruction(DefaultComputeUnits)
			tx, err := s.Solana.Chain.NewTransactionFromInstructions(
				s.SolanaUser.PublicKey(),
				computeBudgetInstruction,
				sendPacketInstruction,
			)
			s.Require().NoError(err)

			solanaSendTxSig, err = s.Solana.Chain.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentFinalized, s.SolanaUser)
			s.Require().NoError(err)

			sendPacketSlot, err = s.Solana.Chain.RPCClient.GetSlot(ctx, rpc.CommitmentFinalized)
			s.Require().NoError(err)

			s.T().Logf("Solana → Cosmos packet sent - tx: %s, base sequence: %d, slot: %d", solanaSendTxSig, solanaBaseSequence, sendPacketSlot)

			sendPacketEvent, err = solana.GetSendPacketEventFromTransaction(ctx, s.Solana.Chain.RPCClient, solanaSendTxSig)
			s.Require().NoError(err)
			s.Require().NotNil(sendPacketEvent)
		}))

		s.Require().True(s.Run("Verify packet commitment exists on Solana", func() {
			namespacedSequence := solana.CalculateNamespacedSequence(solanaBaseSequence, s.TestAppProgramID, s.SolanaUser.PublicKey())
			namespacedSequenceBytes := make([]byte, 8)
			binary.LittleEndian.PutUint64(namespacedSequenceBytes, namespacedSequence)
			packetCommitmentPDA, _ := solana.Ics26Router.PacketCommitmentWithArgSeedPDA(ics26_router.ProgramID, []byte(s.AttestationClientID), namespacedSequenceBytes)

			accountInfo, err := s.Solana.Chain.RPCClient.GetAccountInfoWithOpts(ctx, packetCommitmentPDA, &rpc.GetAccountInfoOpts{
				Commitment: rpc.CommitmentFinalized,
			})
			s.Require().NoError(err)
			s.Require().NotNil(accountInfo.Value)
			s.T().Logf("Solana packet commitment verified for base sequence %d", solanaBaseSequence)
		}))

		s.Require().True(s.Run("Verify Solana attestor can attest packet", func() {
			abiPacket := convertSolanaPacketToABI(sendPacketEvent.Packet)
			packetBytes, err := types.AbiEncodePacket(abiPacket)
			s.Require().NoError(err)

			resp, err := attestor.GetPacketAttestation(ctx, s.SolanaAttestorClient, [][]byte{packetBytes}, sendPacketSlot)
			s.Require().NoError(err)
			s.Require().NotNil(resp.GetAttestation())

			attestation := resp.GetAttestation()
			s.Require().NotEmpty(attestation.GetSignature())
			s.Require().Equal(sendPacketSlot, attestation.GetHeight())

			s.T().Log("Solana attestor packet verification successful")
		}))

		var cosmosRecvTxHash string

		s.Require().True(s.Run("Relay packet to Cosmos", func() {
			resp, err := s.RelayerClient.RelayByTx(ctx, &relayertypes.RelayByTxRequest{
				SrcChain:    testvalues.SolanaChainID,
				DstChain:    simd.Config().ChainID,
				SourceTxIds: [][]byte{[]byte(solanaSendTxSig.String())},
				SrcClientId: s.AttestationClientID,
				DstClientId: CosmosClientID,
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Tx)

			relayTxResult := s.MustBroadcastSdkTxBody(ctx, simd, cosmosUserWallet, CosmosDefaultGasLimit, resp.Tx)
			cosmosRecvTxHash = relayTxResult.TxHash
			s.T().Logf("Packet relayed to Cosmos - tx: %s", cosmosRecvTxHash)
		}))

		s.Require().True(s.Run("Verify packet receipt on Cosmos", func() {
			receiptResp, err := e2esuite.GRPCQuery[channeltypesv2.QueryPacketReceiptResponse](ctx, simd, &channeltypesv2.QueryPacketReceiptRequest{
				ClientId: CosmosClientID,
				Sequence: sendPacketEvent.Sequence,
			})
			s.Require().NoError(err)
			s.Require().True(receiptResp.Received)
			s.T().Logf("Cosmos packet receipt verified for sequence %d", sendPacketEvent.Sequence)
		}))

		s.Require().True(s.Run("Relay ACK to Solana", func() {
			cosmosRecvTxHashBytes, err := hex.DecodeString(cosmosRecvTxHash)
			s.Require().NoError(err)

			resp, err := s.RelayerClient.RelayByTx(ctx, &relayertypes.RelayByTxRequest{
				SrcChain:    simd.Config().ChainID,
				DstChain:    testvalues.SolanaChainID,
				SourceTxIds: [][]byte{cosmosRecvTxHashBytes},
				SrcClientId: CosmosClientID,
				DstClientId: s.AttestationClientID,
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Tx)

			ackTxSig, err := s.Solana.Chain.SubmitChunkedRelayPackets(ctx, s.T(), resp, s.SolanaUser)
			s.Require().NoError(err)
			s.T().Logf("ACK relayed to Solana - tx: %s", ackTxSig)
		}))

		s.Require().True(s.Run("Verify packet commitment deleted on Solana", func() {
			s.Solana.Chain.VerifyPacketCommitmentDeleted(ctx, s.T(), s.Require(), s.AttestationClientID, solanaBaseSequence, s.TestAppProgramID, s.SolanaUser.PublicKey())
			s.T().Logf("Solana packet commitment deleted for base sequence %d", solanaBaseSequence)
		}))
	}))

	s.Require().True(s.Run("Verify final state", func() {
		resp, err := e2esuite.GRPCQuery[banktypes.QueryBalanceResponse](ctx, simd, &banktypes.QueryBalanceRequest{
			Address: cosmosUserAddress,
			Denom:   transferCoin.Denom,
		})
		s.Require().NoError(err)
		s.Require().NotNil(resp.Balance)
		finalBalance := resp.Balance.Amount.Int64()

		s.T().Logf("Final Cosmos balance: %d %s (initial: %d)", finalBalance, transferCoin.Denom, initialCosmosBalance)
		s.T().Log("Roundtrip complete: Cosmos → Solana → Cosmos")
	}))
}

func convertSolanaPacketToABI(packet solana.SolanaPacket) ics26router.IICS26RouterMsgsPacket {
	payloads := make([]ics26router.IICS26RouterMsgsPayload, len(packet.Payloads))
	for i, p := range packet.Payloads {
		payloads[i] = ics26router.IICS26RouterMsgsPayload{
			SourcePort: p.SourcePort,
			DestPort:   p.DestPort,
			Version:    p.Version,
			Encoding:   p.Encoding,
			Value:      p.Value,
		}
	}

	return ics26router.IICS26RouterMsgsPacket{
		Sequence:         packet.Sequence,
		SourceClient:     packet.SourceClient,
		DestClient:       packet.DestClient,
		TimeoutTimestamp: uint64(packet.TimeoutTimestamp),
		Payloads:         payloads,
	}
}

// deriveAttestationConsensusStatePDA fetches the attestation client state to get the latest height,
// then derives the consensus state PDA.
func (s *IbcSolanaAttestationTestSuite) deriveAttestationConsensusStatePDA(ctx context.Context, clientStatePDA solanago.PublicKey) solanago.PublicKey {
	accountInfo, err := s.Solana.Chain.RPCClient.GetAccountInfoWithOpts(ctx, clientStatePDA, &rpc.GetAccountInfoOpts{
		Commitment: rpc.CommitmentConfirmed,
	})
	s.Require().NoError(err)

	clientState, err := attestation.ParseAccount_AttestationTypesClientState(accountInfo.Value.Data.GetBinary())
	s.Require().NoError(err)

	heightBytes := make([]byte, 8)
	binary.LittleEndian.PutUint64(heightBytes, clientState.LatestHeight)

	consensusStatePDA, _ := solana.Attestation.ConsensusStateWithArgSeedPDA(
		attestation.ProgramID,
		heightBytes,
	)
	return consensusStatePDA
}
