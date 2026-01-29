package main

import (
	"context"
	"encoding/binary"
	"encoding/hex"
	"fmt"
	"os"
	"strconv"
	"testing"
	"time"

	dummy_ibc_app "github.com/cosmos/solidity-ibc-eureka/e2e/interchaintestv8/solana/go-anchor/dummyibcapp"
	bin "github.com/gagliardetto/binary"
	"github.com/stretchr/testify/suite"

	solanago "github.com/gagliardetto/solana-go"
	"github.com/gagliardetto/solana-go/rpc"

	sdkmath "cosmossdk.io/math"

	sdk "github.com/cosmos/cosmos-sdk/types"

	transfertypes "github.com/cosmos/ibc-go/v10/modules/apps/transfer/types"
	clienttypesv2 "github.com/cosmos/ibc-go/v10/modules/core/02-client/v2/types"
	channeltypesv2 "github.com/cosmos/ibc-go/v10/modules/core/04-channel/v2/types"
	ibcexported "github.com/cosmos/ibc-go/v10/modules/core/exported"

	"github.com/cosmos/interchaintest/v10/testutil"

	ics26router "github.com/cosmos/solidity-ibc-eureka/packages/go-abigen/ics26router"

	access_manager "github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/accessmanager"
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

// IbcSolanaAttestorTestSuite tests the Solana adapter for the attestor
type IbcSolanaAttestorTestSuite struct {
	e2esuite.TestSuite

	SolanaUser               *solanago.Wallet
	Ics26RouterProgramID     solanago.PublicKey
	DummyAppProgramID        solanago.PublicKey
	Ics07TendermintProgramID solanago.PublicKey
	SolanaAltAddress         string

	AttestorContainers []*attestor.AttestorContainer
	AttestorEndpoints  []string
	AttestorClient     attestortypes.AttestationServiceClient

	RelayerClient  relayertypes.RelayerServiceClient
	RelayerProcess *os.Process
}

func TestWithIbcSolanaAttestorTestSuite(t *testing.T) {
	suite.Run(t, new(IbcSolanaAttestorTestSuite))
}

func (s *IbcSolanaAttestorTestSuite) TearDownSuite() {
	attestor.CleanupContainers(context.Background(), s.T(), s.AttestorContainers)

	if s.RelayerProcess != nil {
		s.T().Logf("Cleaning up relayer process (PID: %d)", s.RelayerProcess.Pid)
		if err := s.RelayerProcess.Kill(); err != nil {
			s.T().Logf("Failed to kill relayer process: %v", err)
		}
	}
}

func (s *IbcSolanaAttestorTestSuite) SetupSuite(ctx context.Context) {
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
			Name: "Deploy Dummy App",
			Run: func() (solanago.PublicKey, error) {
				keypairPath := fmt.Sprintf("%s/dummy_ibc_app-keypair.json", keypairDir)
				return s.Solana.Chain.DeploySolanaProgramAsync(ctx, "dummy_ibc_app", keypairPath, deployerPath)
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
	s.DummyAppProgramID = deployResults["Deploy Dummy App"]
	dummy_ibc_app.ProgramID = s.DummyAppProgramID
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
	altAddress := s.Solana.Chain.CreateIBCAddressLookupTable(
		ctx, s.T(), s.Require(), s.SolanaUser, cosmosChainID, transfertypes.PortID, SolanaClientID,
	)
	s.SolanaAltAddress = altAddress.String()

	s.T().Log("Starting attestor service(s)...")
	attestorResult := attestor.SetupSolanaAttestors(ctx, s.T(), s.GetDockerClient(), s.GetNetworkID(), testvalues.SolanaLocalnetRPC, ics26_router.ProgramID.String())
	s.AttestorContainers = attestorResult.Containers
	s.AttestorEndpoints = attestorResult.Endpoints

	if len(s.AttestorEndpoints) > 0 {
		serverAddr := s.AttestorEndpoints[0][len("http://"):]
		var err error
		s.AttestorClient, err = attestor.GetAttestationServiceClient(serverAddr)
		s.Require().NoError(err)
	}

	s.T().Log("Starting relayer...")

	config := relayer.NewConfigBuilder().
		SolanaToCosmosAttested(relayer.SolanaToCosmosAttestedParams{
			SolanaChainID:     testvalues.SolanaChainID,
			CosmosChainID:     simd.Config().ChainID,
			SolanaRPC:         testvalues.SolanaLocalnetRPC,
			TmRPC:             simd.GetHostRPCAddress(),
			ICS26ProgramID:    ics26_router.ProgramID.String(),
			SignerAddress:     s.Cosmos.Users[0].FormattedAddress(),
			AttestorEndpoints: s.AttestorEndpoints,
			AttestorTimeout:   300000,
			QuorumThreshold:   testvalues.DefaultMinRequiredSigs,
		}).
		CosmosToSolana(relayer.CosmosToSolanaParams{
			CosmosChainID:  simd.Config().ChainID,
			SolanaChainID:  testvalues.SolanaChainID,
			SolanaRPC:      testvalues.SolanaLocalnetRPC,
			TmRPC:          simd.GetHostRPCAddress(),
			ICS07ProgramID: s.Ics07TendermintProgramID.String(),
			ICS26ProgramID: ics26_router.ProgramID.String(),
			FeePayer:       s.SolanaUser.PublicKey().String(),
			ALTAddress:     s.SolanaAltAddress,
			MockClient:     true,
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

	s.T().Log("Creating IBC client on Solana...")
	resp, err := s.RelayerClient.CreateClient(ctx, &relayertypes.CreateClientRequest{
		SrcChain:   simd.Config().ChainID,
		DstChain:   testvalues.SolanaChainID,
		Parameters: map[string]string{},
	})
	s.Require().NoError(err)
	s.Require().NotEmpty(resp.Tx)

	unsignedSolanaTx, err := solanago.TransactionFromDecoder(bin.NewBinDecoder(resp.Tx))
	s.Require().NoError(err)

	sig, err := s.Solana.Chain.SignAndBroadcastTxWithRetry(ctx, unsignedSolanaTx, rpc.CommitmentFinalized, s.SolanaUser)
	s.Require().NoError(err)

	s.T().Logf("IBC client created on Solana - tx: %s", sig)

	s.T().Log("Creating attestor client on Cosmos with 1 attestor...")
	currentFinalizedSlot, err := s.Solana.Chain.RPCClient.GetSlot(ctx, rpc.CommitmentFinalized)
	s.Require().NoError(err)
	solanaTimestamp, err := s.Solana.Chain.RPCClient.GetBlockTime(ctx, currentFinalizedSlot)
	s.Require().NoError(err)

	clientResp, err := s.RelayerClient.CreateClient(ctx, &relayertypes.CreateClientRequest{
		SrcChain: testvalues.SolanaChainID,
		DstChain: simd.Config().ChainID,
		Parameters: map[string]string{
			testvalues.ParameterKey_AttestorAddresses: attestorResult.Addresses[0],
			testvalues.ParameterKey_MinRequiredSigs:   strconv.Itoa(testvalues.DefaultMinRequiredSigs),
			testvalues.ParameterKey_height:            strconv.FormatUint(currentFinalizedSlot, 10),
			testvalues.ParameterKey_timestamp:         strconv.FormatInt(int64(*solanaTimestamp), 10),
		},
	})
	s.Require().NoError(err)

	txResp := s.MustBroadcastSdkTxBody(ctx, simd, s.Cosmos.Users[0], CosmosCreateClientGasLimit, clientResp.Tx)
	s.T().Logf("IBC client created on Cosmos - tx: %s", txResp.TxHash)

	s.T().Log("Adding client to Router on Solana...")
	routerStateAccount, _ = solana.Ics26Router.RouterStatePDA(ics26_router.ProgramID)
	clientAccount, _ := solana.Ics26Router.ClientWithArgSeedPDA(ics26_router.ProgramID, []byte(SolanaClientID))
	clientSequenceAccount, _ := solana.Ics26Router.ClientSequenceWithArgSeedPDA(ics26_router.ProgramID, []byte(SolanaClientID))

	counterpartyInfo := ics26_router.SolanaIbcTypesRouterCounterpartyInfo{
		ClientId:     testvalues.FirstAttestationsClientID,
		MerklePrefix: [][]byte{[]byte(ibcexported.StoreKey), []byte("")},
	}

	addClientInstruction, err := ics26_router.NewAddClientInstruction(
		SolanaClientID,
		counterpartyInfo,
		s.SolanaUser.PublicKey(),
		routerStateAccount,
		accessControlAccount,
		clientAccount,
		clientSequenceAccount,
		s.SolanaUser.PublicKey(),
		ics07_tendermint.ProgramID,
		solanago.SystemProgramID,
		solanago.SysVarInstructionsPubkey,
	)
	s.Require().NoError(err)

	tx, err = s.Solana.Chain.NewTransactionFromInstructions(s.SolanaUser.PublicKey(), addClientInstruction)
	s.Require().NoError(err)

	_, err = s.Solana.Chain.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentFinalized, s.SolanaUser)
	s.Require().NoError(err)
	s.T().Log("Client added to router")

	s.T().Log("Registering counterparty on Cosmos...")
	merklePathPrefix := [][]byte{[]byte("")}
	_, err = s.BroadcastMessages(ctx, simd, s.Cosmos.Users[0], CosmosDefaultGasLimit, &clienttypesv2.MsgRegisterCounterparty{
		ClientId:                 testvalues.FirstAttestationsClientID,
		CounterpartyMerklePrefix: merklePathPrefix,
		CounterpartyClientId:     SolanaClientID,
		Signer:                   s.Cosmos.Users[0].FormattedAddress(),
	})
	s.Require().NoError(err)
	s.T().Log("Counterparty registered on Cosmos")

	s.T().Log("Initializing Dummy IBC App...")
	appStateAccount, _ := solana.DummyIbcApp.AppStateTransferPDA(s.DummyAppProgramID)

	initAppInstruction, err := dummy_ibc_app.NewInitializeInstruction(
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
	s.T().Log("Dummy IBC App initialized successfully")

	s.T().Log("Registering Dummy App with Router...")
	routerStateAccount, _ = solana.Ics26Router.RouterStatePDA(ics26_router.ProgramID)
	ibcAppAccount, _ := solana.Ics26Router.IbcAppWithArgSeedPDA(ics26_router.ProgramID, []byte(transfertypes.PortID))

	registerInstruction, err := ics26_router.NewAddIbcAppInstruction(
		transfertypes.PortID,
		routerStateAccount,
		accessControlAccount,
		ibcAppAccount,
		s.DummyAppProgramID,
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
	s.T().Log("Dummy app registered with router")
}

// convertSolanaPacket converts a Solana packet to ics26router.IICS26RouterMsgsPacket for ABI encoding
func convertSolanaPacket(packet solana.SolanaPacket) ics26router.IICS26RouterMsgsPacket {
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

func (s *IbcSolanaAttestorTestSuite) Test_SolanaAttestor_StateAttestation() {
	ctx := context.Background()
	s.SetupSuite(ctx)

	slot, err := s.Solana.Chain.RPCClient.GetSlot(ctx, rpc.CommitmentFinalized)
	s.Require().NoError(err)

	resp, err := attestor.GetStateAttestation(ctx, s.AttestorClient, slot)
	s.Require().NoError(err)
	s.Require().NotNil(resp.GetAttestation())

	attestation := resp.GetAttestation()
	s.Require().NotEmpty(attestation.GetSignature())
	s.Require().Equal(slot, attestation.GetHeight())
	s.Require().Greater(attestation.GetTimestamp(), uint64(0))

	attestedData := attestation.GetAttestedData()
	s.Require().NotEmpty(attestedData)
}

func (s *IbcSolanaAttestorTestSuite) Test_SolanaAttestor_VerifyPacketCommitments() {
	ctx := context.Background()
	s.SetupSuite(ctx)

	var solanaTxSig solanago.Signature
	var baseSequence uint64
	var namespacedSequence uint64
	var packetCommitmentPDA solanago.PublicKey
	var slot uint64

	s.Require().True(s.Run("Send packet on Solana", func() {
		appState, _ := solana.DummyIbcApp.AppStateTransferPDA(s.DummyAppProgramID)
		routerState, _ := solana.Ics26Router.RouterStatePDA(ics26_router.ProgramID)
		ibcApp, _ := solana.Ics26Router.IbcAppWithArgSeedPDA(ics26_router.ProgramID, []byte(transfertypes.PortID))
		client, _ := solana.Ics26Router.ClientWithArgSeedPDA(ics26_router.ProgramID, []byte(SolanaClientID))
		clientSequence, _ := solana.Ics26Router.ClientSequenceWithArgSeedPDA(ics26_router.ProgramID, []byte(SolanaClientID))

		clientSequenceAccountInfo, err := s.Solana.Chain.RPCClient.GetAccountInfoWithOpts(ctx, clientSequence, &rpc.GetAccountInfoOpts{
			Commitment: rpc.CommitmentFinalized,
		})
		s.Require().NoError(err)

		clientSequenceData, err := ics26_router.ParseAccount_Ics26RouterStateClientSequence(clientSequenceAccountInfo.Value.Data.GetBinary())
		s.Require().NoError(err)
		baseSequence = clientSequenceData.NextSequenceSend

		namespacedSequence = solana.CalculateNamespacedSequence(baseSequence, s.DummyAppProgramID, s.SolanaUser.PublicKey())

		namespacedSequenceBytes := make([]byte, 8)
		binary.LittleEndian.PutUint64(namespacedSequenceBytes, namespacedSequence)
		packetCommitmentPDA, _ = solana.Ics26Router.PacketCommitmentWithArgSeedPDA(ics26_router.ProgramID, []byte(SolanaClientID), namespacedSequenceBytes)

		timeoutTimestamp := time.Now().Add(1 * time.Hour).Unix()
		packetMsg := dummy_ibc_app.DummyIbcAppInstructionsSendPacketSendPacketMsg{
			SourceClient:     SolanaClientID,
			SourcePort:       transfertypes.PortID,
			DestPort:         transfertypes.PortID,
			Version:          transfertypes.V1,
			Encoding:         "application/json",
			PacketData:       []byte(`{"test":"data"}`),
			TimeoutTimestamp: timeoutTimestamp,
		}

		sendPacketInstruction, err := dummy_ibc_app.NewSendPacketInstruction(
			packetMsg,
			appState,
			s.SolanaUser.PublicKey(),
			routerState,
			ibcApp,
			clientSequence,
			packetCommitmentPDA,
			client,
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
		s.Require().Equal(SolanaClientID, event.ClientID)
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

	s.Require().True(s.Run("Call attestor and verify adapter", func() {
		abiPacket := convertSolanaPacket(event.Packet)
		packetBytes, err := types.AbiEncodePacket(abiPacket)
		s.Require().NoError(err)

		resp, err := attestor.GetPacketAttestation(ctx, s.AttestorClient, [][]byte{packetBytes}, slot)
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
		s.Require().Len(packetAttestation.Packets, 1, "Should have exactly one packet")

		attestorCommitment := packetAttestation.Packets[0].Commitment[:]
		s.Require().Equal(onChainCommitment, attestorCommitment)

		s.T().Log("✓ Packet commitment verification successful")
	}))
}

func (s *IbcSolanaAttestorTestSuite) Test_SolanaAttestor_VerifyAckCommitment() {
	ctx := context.Background()
	s.SetupSuite(ctx)

	var cosmosRelayPacketTxHash []byte
	var solanaRelayTxSig solanago.Signature
	var slot uint64

	simd := s.Cosmos.Chains[0]

	var timeout uint64
	var encodedPayload []byte

	s.Require().True(s.Run("Send packet from Cosmos to Solana", func() {
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
			Memo:     "cosmos-to-solana-transfer",
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
			SourceClient:     testvalues.FirstAttestationsClientID,
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

		s.T().Logf("Cosmos packet transaction sent: %s", resp.TxHash)
	}))

	s.Require().NoError(testutil.WaitForBlocks(ctx, 1, simd))

	s.Require().True(s.Run("Relay packet to Solana", func() {
		s.Require().True(s.Run("Update Tendermint client on Solana", func() {
			resp, err := s.RelayerClient.UpdateClient(ctx, &relayertypes.UpdateClientRequest{
				SrcChain:    simd.Config().ChainID,
				DstChain:    testvalues.SolanaChainID,
				DstClientId: SolanaClientID,
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Tx)

			s.Solana.Chain.SubmitChunkedUpdateClient(ctx, s.T(), s.Require(), resp, s.SolanaUser)
		}))

		s.Require().True(s.Run("Relay packet", func() {
			resp, err := s.RelayerClient.RelayByTx(ctx, &relayertypes.RelayByTxRequest{
				SrcChain:    simd.Config().ChainID,
				DstChain:    testvalues.SolanaChainID,
				SourceTxIds: [][]byte{cosmosRelayPacketTxHash},
				SrcClientId: testvalues.FirstAttestationsClientID,
				DstClientId: SolanaClientID,
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Tx)

			solanaRelayTxSig, err = s.Solana.Chain.SubmitChunkedRelayPackets(ctx, s.T(), resp, s.SolanaUser)
			s.Require().NoError(err)

			slot, err = s.Solana.Chain.RPCClient.GetSlot(ctx, rpc.CommitmentFinalized)
			s.Require().NoError(err)
		}))
	}))

	actualSequence := uint64(1)

	sequenceBytes := make([]byte, 8)
	binary.LittleEndian.PutUint64(sequenceBytes, actualSequence)

	s.Require().NoError(s.Solana.Chain.WaitForTxStatus(solanaRelayTxSig, rpc.ConfirmationStatusFinalized))

	var onChainAckCommitment []byte

	s.Require().True(s.Run("Query ACK commitment on chain", func() {
		packetAckPDA, _ := solana.Ics26Router.PacketAckWithArgSeedPDA(ics26_router.ProgramID, []byte(SolanaClientID), sequenceBytes)

		accountInfo, err := s.Solana.Chain.RPCClient.GetAccountInfoWithOpts(ctx, packetAckPDA, &rpc.GetAccountInfoOpts{
			Commitment: rpc.CommitmentFinalized,
		})
		s.Require().NoError(err)
		s.Require().NotNil(accountInfo.Value)

		data := accountInfo.Value.Data.GetBinary()
		s.Require().GreaterOrEqual(len(data), 40)

		onChainAckCommitment = data[8:40]
		s.Require().NotEmpty(onChainAckCommitment)
	}))

	s.Require().True(s.Run("Verify ACK commitment via attestor", func() {
		abiPacket := ics26router.IICS26RouterMsgsPacket{
			Sequence:         1,
			SourceClient:     testvalues.FirstAttestationsClientID,
			DestClient:       SolanaClientID,
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

		resp, err := attestor.GetPacketAttestation(ctx, s.AttestorClient, [][]byte{packetBytes}, slot, attestortypes.CommitmentType_COMMITMENT_TYPE_ACK)
		s.Require().NoError(err)
		s.Require().NotNil(resp.GetAttestation())

		attestation := resp.GetAttestation()
		s.Require().NotEmpty(attestation.GetSignature())
		s.Require().Equal(slot, attestation.GetHeight())

		attestedData := attestation.GetAttestedData()

		packetAttestation, err := types.AbiDecodePacketAttestation(attestedData)
		s.Require().NoError(err)
		s.Require().NotNil(packetAttestation)
		s.Require().Len(packetAttestation.Packets, 1, "Should have exactly one packet")

		attestorAckCommitment := packetAttestation.Packets[0].Commitment[:]
		s.Require().Equal(onChainAckCommitment, attestorAckCommitment)
	}))
}
