package main

import (
	"context"
	"crypto/ecdsa"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"math/big"
	"os"
	"strconv"
	"strings"
	"testing"
	"time"
	"unicode"

	dockerclient "github.com/docker/docker/client"
	"github.com/stretchr/testify/require"
	"github.com/stretchr/testify/suite"
	"go.uber.org/zap"
	"go.uber.org/zap/zaptest"

	"github.com/ethereum/go-ethereum/accounts/abi/bind"
	ethcommon "github.com/ethereum/go-ethereum/common"
	ethtypes "github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/crypto"
	"github.com/ethereum/go-ethereum/ethclient"

	errorsmod "cosmossdk.io/errors"
	sdkmath "cosmossdk.io/math"

	"github.com/cosmos/cosmos-sdk/client"
	"github.com/cosmos/cosmos-sdk/client/tx"
	sdk "github.com/cosmos/cosmos-sdk/types"
	authtypes "github.com/cosmos/cosmos-sdk/x/auth/types"
	banktypes "github.com/cosmos/cosmos-sdk/x/bank/types"
	govtypes "github.com/cosmos/cosmos-sdk/x/gov/types"
	govtypesv1 "github.com/cosmos/cosmos-sdk/x/gov/types/v1"

	ibcwasmtypes "github.com/cosmos/ibc-go/modules/light-clients/08-wasm/types"
	transfertypes "github.com/cosmos/ibc-go/v8/modules/apps/transfer/types"
	clienttypes "github.com/cosmos/ibc-go/v8/modules/core/02-client/types"
	channeltypes "github.com/cosmos/ibc-go/v8/modules/core/04-channel/types"
	commitmenttypes "github.com/cosmos/ibc-go/v8/modules/core/23-commitment/types"
	ibchost "github.com/cosmos/ibc-go/v8/modules/core/24-host"
	ibcexported "github.com/cosmos/ibc-go/v8/modules/core/exported"
	ibctesting "github.com/cosmos/ibc-go/v8/testing"

	interchaintest "github.com/strangelove-ventures/interchaintest/v8"
	"github.com/strangelove-ventures/interchaintest/v8/chain/cosmos"
	"github.com/strangelove-ventures/interchaintest/v8/ibc"
	"github.com/strangelove-ventures/interchaintest/v8/testreporter"
	"github.com/strangelove-ventures/interchaintest/v8/testutil"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/chainconfig"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/e2esuite"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/ethereum"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/operator"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/erc20"
	ethereumligthclient "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/ethereumlightclient"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/ics02client"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/ics20transfer"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/ics26router"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/sp1ics07tendermint"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/visualizerclient"
)

const visualizerPort = 6969

// TestSuite is a suite of tests that require two chains and a relayer
type FastSuite struct {
	suite.Suite

	ChainA           ethereum.Ethereum
	ChainB           *cosmos.CosmosChain
	UserB            ibc.Wallet
	dockerClient     *dockerclient.Client
	network          string
	logger           *zap.Logger
	ExecRep          *testreporter.RelayerExecReporter
	VisualizerClient *visualizerclient.VisualizerClient

	// proposalIDs keeps track of the active proposal ID for cosmos chains
	proposalIDs map[string]uint64

	// Whether to generate fixtures for the solidity tests
	generateFixtures bool

	// The private key of a test account
	key *ecdsa.PrivateKey
	// The private key of the faucet account of interchaintest
	deployer *ecdsa.PrivateKey

	contractAddresses ethereum.DeployedContracts

	sp1Ics07Contract   *sp1ics07tendermint.Contract
	ics02Contract      *ics02client.Contract
	ics26Contract      *ics26router.Contract
	ics20Contract      *ics20transfer.Contract
	erc20Contract      *erc20.Contract
	escrowContractAddr ethcommon.Address

	// The (hex encoded) checksum of the ethereum wasm client contract deployed on the Cosmos chain
	unionClientChecksum     string
	unionClientID           string
	tendermintClientID      string
	spec                    ethereum.Spec
	initialNextSyncComittee ethereum.SyncCommittee
	initialConsHeight       uint64
}

// SetupSuite sets up the chains, relayer, user accounts, clients, and connections
func (s *FastSuite) SetupSuite(ctx context.Context) {
	t := s.T()

	s.VisualizerClient = visualizerclient.NewVisualizerClient(visualizerPort, t.Name())
	s.LogVisualizerMessage("FastSuite setup started")
	chainSpecs := chainconfig.DefaultChainSpecs

	t.Cleanup(func() {
		// ctx := context.Background()
		if t.Failed() {
			s.LogVisualizerMessage("Test failed")
			// s.ChainA.DumpLogs(ctx)
		}
		s.LogVisualizerMessage("Test run done and cleanup completed")
	})

	if len(chainSpecs) != 1 {
		t.Fatal("FastSuite requires exactly 1 chain spec")
	}

	s.logger = zaptest.NewLogger(t)
	s.dockerClient, s.network = interchaintest.DockerSetup(t)

	cf := interchaintest.NewBuiltinChainFactory(s.logger, chainSpecs)

	chains, err := cf.Chains(t.Name())
	s.Require().NoError(err)

	// s.ChainA, err = ethereum.ConnectToRunningEthereum(ctx)
	s.ChainA, err = ethereum.SpinUpEthereum(ctx)

	s.Require().NoError(err)
	s.ChainB = chains[0].(*cosmos.CosmosChain)

	s.ExecRep = testreporter.NewNopReporter().RelayerExecReporter(t)

	ic := interchaintest.NewInterchain().
		AddChain(s.ChainB)

	s.Require().NoError(ic.Build(ctx, s.ExecRep, interchaintest.InterchainBuildOptions{
		TestName:         t.Name(),
		Client:           s.dockerClient,
		NetworkID:        s.network,
		SkipPathCreation: true,
	}))

	s.LogVisualizerMessage(fmt.Sprintf("Chains started: %s, %s", s.ChainA.ChainID.String(), s.ChainB.Config().ChainID))

	// map all query request types to their gRPC method paths for cosmos chains
	s.Require().NoError(e2esuite.PopulateQueryReqToPath(ctx, s.ChainB))

	// Fund user accounts
	cosmosUserFunds := sdkmath.NewInt(testvalues.InitialBalance)
	cosmosUsers := interchaintest.GetAndFundTestUsers(t, ctx, t.Name(), cosmosUserFunds, s.ChainB)
	s.UserB = cosmosUsers[0]

	s.proposalIDs = make(map[string]uint64)
	s.proposalIDs[s.ChainB.Config().ChainID] = 1
}

func TestWithFastSuite(t *testing.T) {
	suite.Run(t, new(FastSuite))
}

func (s *FastSuite) TestFastShit() {
	ctx := context.Background()
	s.SetupSuite(ctx)
	s.LogVisualizerMessage("Testing some fast shit")

	eth, simd := s.ChainA, s.ChainB

	var prover string
	s.Require().True(s.Run("Set up environment", func() {
		err := os.Chdir("../..")
		s.Require().NoError(err)

		s.key, err = eth.CreateAndFundUser()
		s.Require().NoError(err)

		operatorKey, err := eth.CreateAndFundUser()
		s.Require().NoError(err)

		s.deployer, err = eth.CreateAndFundUser()
		s.Require().NoError(err)

		prover = os.Getenv(testvalues.EnvKeySp1Prover)
		switch prover {
		case "":
			prover = testvalues.EnvValueSp1Prover_Network
		case testvalues.EnvValueSp1Prover_Mock:
			s.T().Logf("Using mock prover")
		case testvalues.EnvValueSp1Prover_Network:
		default:
			s.Require().Fail("invalid prover type: %s", prover)
		}

		os.Setenv(testvalues.EnvKeyRustLog, testvalues.EnvValueRustLog_Info)
		os.Setenv(testvalues.EnvKeyEthRPC, eth.RPC)
		os.Setenv(testvalues.EnvKeyTendermintRPC, simd.GetHostRPCAddress())
		os.Setenv(testvalues.EnvKeySp1Prover, prover)
		os.Setenv(testvalues.EnvKeyOperatorPrivateKey, hex.EncodeToString(crypto.FromECDSA(operatorKey)))
		if os.Getenv(testvalues.EnvKeyGenerateFixtures) == testvalues.EnvValueGenerateFixtures_True {
			s.generateFixtures = true
		}
	}))

	s.Require().True(s.Run("Deploy ethereum contracts", func() {
		s.Require().NoError(operator.RunGenesis(
			"--trust-level", testvalues.DefaultTrustLevel.String(),
			"--trusting-period", strconv.Itoa(testvalues.DefaultTrustPeriod),
			"-o", testvalues.Sp1GenesisFilePath,
		))

		var (
			stdout []byte
			err    error
		)
		switch prover {
		case testvalues.EnvValueSp1Prover_Mock:
			stdout, err = eth.ForgeScript(s.deployer, "scripts/MockE2ETestDeploy.s.sol:MockE2ETestDeploy")
			s.Require().NoError(err)
		case testvalues.EnvValueSp1Prover_Network:
			// make sure that the SP1_PRIVATE_KEY is set.
			s.Require().NotEmpty(os.Getenv(testvalues.EnvKeySp1PrivateKey))

			stdout, err = eth.ForgeScript(s.deployer, "scripts/E2ETestDeploy.s.sol:E2ETestDeploy")
			s.Require().NoError(err)
		default:
			s.Require().Fail("invalid prover type: %s", prover)
		}

		ethClient, err := ethclient.Dial(eth.RPC)
		s.Require().NoError(err)

		s.contractAddresses, err = ethereum.GetEthContractsFromDeployOutput(string(stdout))
		s.Require().NoError(err)
		s.sp1Ics07Contract, err = sp1ics07tendermint.NewContract(ethcommon.HexToAddress(s.contractAddresses.Ics07Tendermint), ethClient)
		s.Require().NoError(err)
		s.ics02Contract, err = ics02client.NewContract(ethcommon.HexToAddress(s.contractAddresses.Ics02Client), ethClient)
		s.Require().NoError(err)
		s.ics26Contract, err = ics26router.NewContract(ethcommon.HexToAddress(s.contractAddresses.Ics26Router), ethClient)
		s.Require().NoError(err)
		s.ics20Contract, err = ics20transfer.NewContract(ethcommon.HexToAddress(s.contractAddresses.Ics20Transfer), ethClient)
		s.Require().NoError(err)
		s.erc20Contract, err = erc20.NewContract(ethcommon.HexToAddress(s.contractAddresses.Erc20), ethClient)
		s.Require().NoError(err)
		s.escrowContractAddr = ethcommon.HexToAddress(s.contractAddresses.Escrow)

	}))

	s.T().Cleanup(func() {
		_ = os.Remove(testvalues.Sp1GenesisFilePath)
	})

	s.Require().True(s.Run("Fund address with ERC20", func() {
		tx, err := s.erc20Contract.Transfer(s.GetTransactOpts(eth.Faucet), crypto.PubkeyToAddress(s.key.PublicKey), big.NewInt(testvalues.InitialBalance))
		s.Require().NoError(err)

		_ = s.GetTxReciept(ctx, eth, tx.Hash()) // wait for the tx to be mined
	}))

	_, simdRelayerUser := s.GetRelayerUsers(ctx)

	s.Require().True(s.Run("Add ethereum light client on Cosmos chain", func() {
		file, err := os.Open("e2e/interchaintestv8/wasm/ethereum_light_client_minimal.wasm.gz")
		s.Require().NoError(err)

		s.unionClientChecksum = s.PushNewWasmClientProposal(ctx, simd, simdRelayerUser, file)
		s.Require().NotEmpty(s.unionClientChecksum, "checksum was empty but should not have been")

		genesis, err := eth.BeaconAPIClient.GetGenesis()
		s.Require().NoError(err)
		s.spec, err = eth.BeaconAPIClient.GetSpec()
		s.Require().NoError(err)

		_, actualBlockNumber, err := eth.EthAPI.GetBlockNumber()
		s.Require().NoError(err)
		s.LogVisualizerMessage(fmt.Sprintf("creating client: actualBlockNumber: %d", actualBlockNumber))

		executionHeight, err := eth.BeaconAPIClient.GetExecutionHeight("finalized")
		s.Require().NoError(err)
		s.LogVisualizerMessage(fmt.Sprintf("creating client: executionHeight: %d", executionHeight))
		executionNumberHex := fmt.Sprintf("0x%x", executionHeight)

		ethClientState := ethereumligthclient.ClientState{
			ChainId:                      eth.ChainID.String(),
			GenesisValidatorsRoot:        genesis.GenesisValidatorsRoot[:],
			MinSyncCommitteeParticipants: 0,
			GenesisTime:                  uint64(genesis.GenesisTime.Unix()),
			ForkParameters:               s.spec.ToForkParameters(),
			SecondsPerSlot:               uint64(s.spec.SecondsPerSlot.Seconds()),
			SlotsPerEpoch:                s.spec.SlotsPerEpoch,
			EpochsPerSyncCommitteePeriod: s.spec.EpochsPerSyncCommitteePeriod,
			LatestSlot:                   executionHeight,
			FrozenHeight: &clienttypes.Height{
				RevisionNumber: 0,
				RevisionHeight: 0,
			},
			IbcCommitmentSlot:  []byte{0, 0, 0, 0},
			IbcContractAddress: ethcommon.FromHex(ics26RouterAddress),
		}

		ethClientStateBz := simd.Config().EncodingConfig.Codec.MustMarshal(&ethClientState)
		wasmClientChecksum, err := hex.DecodeString(s.unionClientChecksum)
		s.Require().NoError(err)
		latestHeightSlot := clienttypes.Height{
			RevisionNumber: 0,
			RevisionHeight: executionHeight,
		}
		clientState := ibcwasmtypes.ClientState{
			Data:         ethClientStateBz,
			Checksum:     wasmClientChecksum,
			LatestHeight: latestHeightSlot,
		}
		clientStateAny, err := clienttypes.PackClientState(&clientState)
		s.Require().NoError(err)

		proofOfIBCContract, err := eth.EthAPI.GetProof(ics26RouterAddress, []string{}, executionNumberHex)
		s.Require().NoError(err)

		// header, err := eth.BeaconAPIClient.GetHeader(int64(blockNumber))
		header, err := eth.BeaconAPIClient.GetHeader(strconv.Itoa(int(executionHeight)))
		s.Require().NoError(err)
		s.LogVisualizerMessage(fmt.Sprintf("creating client: header for bootstrap %+v", header))
		bootstrap, err := eth.BeaconAPIClient.GetBootstrap(header.Root)
		s.Require().NoError(err)

		//        assert!(bootstrap.header.beacon.slot == height.revision_height);
		if bootstrap.Data.Header.Beacon.Slot != executionHeight {
			s.Require().Fail(fmt.Sprintf("creating client: expected exec height %d, to equal boostrap slot %d", executionHeight, bootstrap.Data.Header.Beacon.Slot))
		}

		timestamp := bootstrap.Data.Header.Execution.Timestamp * 1_000_000_000
		stateRoot := ethereum.HexToBeBytes(bootstrap.Data.Header.Execution.StateRoot)

		s.LogVisualizerMessage(fmt.Sprintf("creating client: bootstrap sync committee aggpubkey: %s", bootstrap.Data.CurrentSyncCommittee.AggregatePubkey))

		currentPeriod := executionHeight / s.spec.Period()
		s.LogVisualizerMessage(fmt.Sprintf("creating client: spec period: %d, current period: %d", s.spec.Period(), currentPeriod))
		clientUpdates, err := eth.BeaconAPIClient.GetLightClientUpdates(currentPeriod, 0)
		s.Require().NoError(err)
		s.Require().NotEmpty(clientUpdates)
		s.LogVisualizerMessage(fmt.Sprintf("create client: clientUpdates len: %d", len(clientUpdates)))
		for _, update := range clientUpdates {
			s.LogVisualizerMessage(fmt.Sprintf("creating client: client update slot: %d", update.Data.AttestedHeader.Beacon.Slot))
			s.LogVisualizerMessage(fmt.Sprintf("creating client: client update next sync c aggpubk: %s", update.Data.NextSyncCommittee.AggregatePubkey))
		}
		update := clientUpdates[0]
		// latestSlot := update.Data.AttestedHeader.Beacon.Slot

		s.initialNextSyncComittee = update.Data.NextSyncCommittee
		s.initialConsHeight = bootstrap.Data.Header.Beacon.Slot
		ethConsensusState := ethereumligthclient.ConsensusState{
			Slot:                 bootstrap.Data.Header.Beacon.Slot,
			StateRoot:            stateRoot,
			StorageRoot:          ethereum.HexToBeBytes(proofOfIBCContract.StorageHash),
			Timestamp:            timestamp,
			CurrentSyncCommittee: ethcommon.FromHex(bootstrap.Data.CurrentSyncCommittee.AggregatePubkey),
			NextSyncCommittee:    ethcommon.FromHex(clientUpdates[0].Data.NextSyncCommittee.AggregatePubkey),
		}

		ethConsensusStateBz := simd.Config().EncodingConfig.Codec.MustMarshal(&ethConsensusState)
		consensusState := ibcwasmtypes.ConsensusState{
			Data: ethConsensusStateBz,
		}
		consensusStateAny, err := clienttypes.PackConsensusState(&consensusState)
		s.Require().NoError(err)

		res, err := s.BroadcastMessages(ctx, simd, simdRelayerUser, 200_000, &clienttypes.MsgCreateClient{
			ClientState:      clientStateAny,
			ConsensusState:   consensusStateAny,
			Signer:           simdRelayerUser.FormattedAddress(),
			CounterpartyId:   "",
			MerklePathPrefix: nil,
		})
		s.Require().NoError(err)

		s.unionClientID, err = ibctesting.ParseClientIDFromEvents(res.Events)
		s.Require().NoError(err)
		s.Require().Equal("08-wasm-0", s.unionClientID)
	}))

	s.Require().True(s.Run("Add client and counterparty on EVM", func() {
		counterpartyInfo := ics02client.IICS02ClientMsgsCounterpartyInfo{
			ClientId:     s.unionClientID,
			MerklePrefix: [][]byte{[]byte(ibcexported.StoreKey), []byte("")},
		}
		lightClientAddress := ethcommon.HexToAddress(s.contractAddresses.Ics07Tendermint)
		tx, err := s.ics02Contract.AddClient(s.GetTransactOpts(s.key), ibcexported.Tendermint, counterpartyInfo, lightClientAddress)
		s.Require().NoError(err)

		receipt := s.GetTxReciept(ctx, eth, tx.Hash())
		event, err := e2esuite.GetEvmEvent(receipt, s.ics02Contract.ParseICS02ClientAdded)
		s.Require().NoError(err)
		s.Require().Equal(ibctesting.FirstClientID, event.ClientId)
		s.Require().Equal(s.unionClientID, event.CounterpartyInfo.ClientId)
		s.tendermintClientID = event.ClientId
	}))

	s.Require().True(s.Run("Register counterparty on Cosmos chain", func() {
		merklePathPrefix := commitmenttypes.NewMerklePath([]byte(""))

		_, err := s.BroadcastMessages(ctx, simd, simdRelayerUser, 200_000, &clienttypes.MsgProvideCounterparty{
			ClientId:         s.unionClientID,
			CounterpartyId:   s.tendermintClientID,
			MerklePathPrefix: &merklePathPrefix,
			Signer:           simdRelayerUser.FormattedAddress(),
		})
		s.Require().NoError(err)
	}))

	ics20Address := ethcommon.HexToAddress(s.contractAddresses.Ics20Transfer)
	transferAmount := big.NewInt(testvalues.TransferAmount)
	ethereumUserAddress := crypto.PubkeyToAddress(s.key.PublicKey)
	cosmosUserWallet := s.UserB
	cosmosUserAddress := cosmosUserWallet.FormattedAddress()

	s.Require().True(s.Run("Approve the ICS20Transfer.sol contract to spend the erc20 tokens", func() {
		tx, err := s.erc20Contract.Approve(s.GetTransactOpts(s.key), ics20Address, transferAmount)
		s.Require().NoError(err)
		receipt := s.GetTxReciept(ctx, eth, tx.Hash())
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)

		allowance, err := s.erc20Contract.Allowance(nil, ethereumUserAddress, ics20Address)
		s.Require().NoError(err)
		s.Require().Equal(transferAmount, allowance)
	}))

	var sendPacket ics26router.IICS26RouterMsgsPacket
	var sendBlockNumber int64
	var commitEvent *ics26router.ContractPacketCommitted
	s.Require().True(s.Run("sendTransfer on Ethereum", func() {
		timeout := uint64(time.Now().Add(30 * time.Minute).Unix())
		msgSendTransfer := ics20transfer.IICS20TransferMsgsSendTransferMsg{
			Denom:            s.contractAddresses.Erc20,
			Amount:           transferAmount,
			Receiver:         cosmosUserAddress,
			SourceChannel:    s.tendermintClientID,
			DestPort:         transfertypes.PortID,
			TimeoutTimestamp: timeout,
			Memo:             "",
		}

		tx, err := s.ics20Contract.SendTransfer(s.GetTransactOpts(s.key), msgSendTransfer)
		s.Require().NoError(err)
		receipt := s.GetTxReciept(ctx, eth, tx.Hash())
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)
		sendBlockNumber = receipt.BlockNumber.Int64()

		transferEvent, err := e2esuite.GetEvmEvent(receipt, s.ics20Contract.ParseICS20Transfer)
		s.Require().NoError(err)
		s.Require().Equal(s.contractAddresses.Erc20, strings.ToLower(transferEvent.Erc20Address.Hex()))
		s.Require().Equal(transferAmount, transferEvent.PacketData.Amount) // converted from erc20 amount to sdk coin amount
		s.Require().Equal(strings.ToLower(ethereumUserAddress.Hex()), strings.ToLower(transferEvent.PacketData.Sender))
		s.Require().Equal(cosmosUserAddress, transferEvent.PacketData.Receiver)
		s.Require().Equal("", transferEvent.PacketData.Memo)

		commitEvent, err = e2esuite.GetEvmEvent(receipt, s.ics26Contract.ParsePacketCommitted)
		s.Require().NoError(err)

		sendPacketEvent, err := e2esuite.GetEvmEvent(receipt, s.ics26Contract.ParseSendPacket)
		s.Require().NoError(err)
		sendPacket = sendPacketEvent.Packet
		s.Require().Equal(uint32(1), sendPacket.Sequence)
		s.Require().Equal(timeout, sendPacket.TimeoutTimestamp)
		s.Require().Equal(transfertypes.PortID, sendPacket.SourcePort)
		s.Require().Equal(s.tendermintClientID, sendPacket.SourceChannel)
		s.Require().Equal(transfertypes.PortID, sendPacket.DestPort)
		s.Require().Equal(s.unionClientID, sendPacket.DestChannel)
		s.Require().Equal(transfertypes.Version, sendPacket.Version)

		s.True(s.Run("Verify balances on Ethereum", func() {
			// User balance on Ethereum
			userBalance, err := s.erc20Contract.BalanceOf(nil, ethereumUserAddress)
			s.Require().NoError(err)
			s.Require().Equal(testvalues.InitialBalance-testvalues.TransferAmount, userBalance.Int64())

			// ICS20 contract balance on Ethereum
			escrowBalance, err := s.erc20Contract.BalanceOf(nil, s.escrowContractAddr)
			s.Require().NoError(err)
			s.Require().Equal(transferAmount, escrowBalance)
		}))
	}))

	// TODO: If packet is not found, we might need to save the current height and make sure we update to one after it

	var lastUnionUpdate uint64
	s.Require().True(s.Run("Update client on Cosmos chain", func() {
		_, updateTo, err := eth.EthAPI.GetBlockNumber()
		s.Require().NoError(err)
		s.LogVisualizerMessage(fmt.Sprintf("first updateTo: %d", updateTo))
		s.LogVisualizerMessage(fmt.Sprintf("sendBlockNumber: %d", sendBlockNumber))

		if updateTo <= sendBlockNumber {
			time.Sleep(30 * time.Second)

			_, updateTo, err = eth.EthAPI.GetBlockNumber()
			s.Require().NoError(err)
			s.Require().Greater(updateTo, sendBlockNumber)
		}

		wasmClientStateDoNotUseMe, _ := s.GetUnionClientState(ctx, s.unionClientID)
		s.LogVisualizerMessage(fmt.Sprintf("wasmClientStateDoNotUseMe latest height: %+v", wasmClientStateDoNotUseMe.LatestHeight.RevisionHeight))
		_, unionConsensusState := s.GetUnionConsensusState(ctx, s.unionClientID, clienttypes.Height{
			RevisionNumber: 0,
			RevisionHeight: s.initialConsHeight,
		})
		s.LogVisualizerMessage(fmt.Sprintf("trusted slot (union cons slot): %d", unionConsensusState.Slot))
		spec, err := eth.BeaconAPIClient.GetSpec()
		s.Require().NoError(err)

		trustedPeriod := unionConsensusState.Slot / spec.Period()
		s.LogVisualizerMessage(fmt.Sprintf("spec period: %d", spec.Period()))
		s.LogVisualizerMessage(fmt.Sprintf("trusted period: %d", trustedPeriod))

		var finalityUpdate ethereum.FinalityUpdateJSONResponse
		var targetPeriod uint64
		err = testutil.WaitForCondition(8*time.Minute, 5*time.Second, func() (bool, error) {
			finalityUpdate, err = eth.BeaconAPIClient.GetFinalityUpdate()
			s.Require().NoError(err)
			targetPeriod = finalityUpdate.Data.AttestedHeader.Beacon.Slot / spec.Period()

			s.LogVisualizerMessage(fmt.Sprintf("Waiting for finality update and target period. updateTo: %d finality update slot: %d, target period: %d", updateTo, finalityUpdate.Data.FinalizedHeader.Beacon.Slot, targetPeriod))

			lightClientUpdates, err := eth.BeaconAPIClient.GetLightClientUpdates(trustedPeriod+1, targetPeriod-trustedPeriod)
			s.Require().NoError(err)

			return len(lightClientUpdates) > 1 && finalityUpdate.Data.FinalizedHeader.Beacon.Slot > uint64(updateTo) && targetPeriod >= trustedPeriod, nil
			// return finalityUpdate.Data.AttestedHeader.Beacon.Slot > uint64(updateTo) && targetPeriod >= trustedPeriod, nil
		})
		s.Require().NoError(err)

		s.LogVisualizerMessage(fmt.Sprintf("targetPeriod: %d", targetPeriod))
		s.LogVisualizerMessage(fmt.Sprintf("trustedPeriod: %d", trustedPeriod))

		// TODO: Try to wait for target period and also light client updates to be 2
		var lightClientUpdates ethereum.LightClientUpdatesResponse
		if trustedPeriod < targetPeriod {
			lightClientUpdates, err = eth.BeaconAPIClient.GetLightClientUpdates(trustedPeriod+1, targetPeriod-trustedPeriod)
			s.Require().NoError(err)
			s.LogVisualizerMessage(fmt.Sprintf("Num light client updates for header updates: %d", len(lightClientUpdates)))
			for _, update := range lightClientUpdates {
				s.LogVisualizerMessage(fmt.Sprintf("light client update for header update slot: %d", update.Data.AttestedHeader.Beacon.Slot))
			}
		} else {
			s.LogVisualizerMessage("No light client updates for header updates")
			lightClientUpdates = []ethereum.LightClientUpdateJSON{}
		}

		newHeaders := []ethereumligthclient.Header{}
		trustedSlot := unionConsensusState.Slot
		oldTrustedSlot := trustedSlot
		for _, update := range lightClientUpdates {
			s.LogVisualizerMessage(fmt.Sprintf("old trusted slot: %d", oldTrustedSlot))

			previousPeriod := uint64(1)
			if update.Data.AttestedHeader.Beacon.Slot/spec.Period() > 1 {
				previousPeriod = update.Data.AttestedHeader.Beacon.Slot / spec.Period()
			}
			previousPeriod -= 1
			s.LogVisualizerMessage(fmt.Sprintf("previous period: %d", previousPeriod))

			executionHeight, err := eth.BeaconAPIClient.GetExecutionHeight(strconv.Itoa(int(update.Data.AttestedHeader.Beacon.Slot)))
			s.Require().NoError(err)
			executionHeightHex := fmt.Sprintf("0x%x", executionHeight)
			s.LogVisualizerMessage(fmt.Sprintf("Execution height: %d", executionHeight))
			proofResp, err := eth.EthAPI.GetProof(s.contractAddresses.Ics26Router, []string{}, executionHeightHex)
			s.Require().NoError(err)
			s.Require().NotEmpty(proofResp.AccountProof)
			s.LogVisualizerMessage(fmt.Sprintf("final update: proof resp: %+v", proofResp))

			var proofBz [][]byte
			for _, proofStr := range proofResp.AccountProof {
				proofBz = append(proofBz, ethcommon.FromHex(proofStr))
				// proofBz = append(proofBz, []byte(proofStr))
			}
			accountUpdate := ethereumligthclient.AccountUpdate{
				AccountProof: &ethereumligthclient.AccountProof{
					StorageRoot: ethereum.HexToBeBytes(proofResp.StorageHash),
					Proof:       proofBz,
				},
			}

			previousLightClientUpdates, err := eth.BeaconAPIClient.GetLightClientUpdates(previousPeriod, 1)
			s.Require().NoError(err)
			s.LogVisualizerMessage(fmt.Sprintf("Num previous light client updates: %d", len(previousLightClientUpdates)))
			for _, previousLightClientUpdate := range previousLightClientUpdates {
				s.LogVisualizerMessage(fmt.Sprintf("prev light client update slot: %d", previousLightClientUpdate.Data.AttestedHeader.Beacon.Slot))
				s.LogVisualizerMessage(fmt.Sprintf("prev light client update next sync c aggpubkey: %s", previousLightClientUpdate.Data.NextSyncCommittee.AggregatePubkey))
			}

			// previousLightClientUpdate := previousLightClientUpdates[len(previousLightClientUpdates)-1]
			previousLightClientUpdate := previousLightClientUpdates[0]
			s.LogVisualizerMessage(fmt.Sprintf("prev light client update slot: %d", previousLightClientUpdate.Data.AttestedHeader.Beacon.Slot))

			var nextSyncCommitteePubkeys [][]byte
			for _, pubkey := range previousLightClientUpdate.Data.NextSyncCommittee.Pubkeys {
				nextSyncCommitteePubkeys = append(nextSyncCommitteePubkeys, ethcommon.FromHex(pubkey))
			}

			consensusUpdate := update.ToLightClientUpdate()
			newHeaders = append(newHeaders, ethereumligthclient.Header{
				ConsensusUpdate: &consensusUpdate,
				TrustedSyncCommittee: &ethereumligthclient.TrustedSyncCommittee{
					TrustedHeight: &clienttypes.Height{
						RevisionNumber: 0,
						RevisionHeight: oldTrustedSlot,
					},
					NextSyncCommittee: &ethereumligthclient.SyncCommittee{
						Pubkeys:         nextSyncCommitteePubkeys,
						AggregatePubkey: ethcommon.FromHex(previousLightClientUpdate.Data.NextSyncCommittee.AggregatePubkey),
					},
					// CurrentSyncCommittee: &ethereumligthclient.SyncCommittee{},
				},
				AccountUpdate: &accountUpdate,
			})

			lastUnionUpdate = oldTrustedSlot
			oldTrustedSlot = update.Data.AttestedHeader.Beacon.Slot
		}

		if trustedPeriod >= targetPeriod {
			newHeaders = []ethereumligthclient.Header{}
		}

		s.LogVisualizerMessage(fmt.Sprintf("final update: finality update slot: %d, spec period: %d", finalityUpdate.Data.AttestedHeader.Beacon.Slot, spec.Period()))
		previousPeriod := (finalityUpdate.Data.AttestedHeader.Beacon.Slot / spec.Period())
		if previousPeriod != 0 {
			previousPeriod -= 1
		}
		s.LogVisualizerMessage(fmt.Sprintf("final update: previous period: %d", previousPeriod))
		executionHeight, err := eth.BeaconAPIClient.GetExecutionHeight(fmt.Sprintf("%d", finalityUpdate.Data.AttestedHeader.Beacon.Slot))
		s.Require().NoError(err)
		s.LogVisualizerMessage(fmt.Sprintf("final update: execution height: %d", executionHeight))
		executionHeightHex := fmt.Sprintf("0x%x", executionHeight)
		proofResp, err := eth.EthAPI.GetProof(s.contractAddresses.Ics26Router, []string{}, executionHeightHex)
		s.Require().NoError(err)
		s.Require().NotEmpty(proofResp.AccountProof)
		s.LogVisualizerMessage(fmt.Sprintf("final update: proof resp: %+v", proofResp))

		var proofBz [][]byte
		for _, proofStr := range proofResp.AccountProof {
			proofBz = append(proofBz, ethcommon.FromHex(proofStr))
		}
		accountUpdate := ethereumligthclient.AccountUpdate{
			AccountProof: &ethereumligthclient.AccountProof{
				StorageRoot: ethereum.HexToBeBytes(proofResp.StorageHash),
				Proof:       proofBz,
			},
		}

		previousPeriodLightClientUpdate, err := eth.BeaconAPIClient.GetLightClientUpdates(previousPeriod, 1)
		s.Require().NoError(err)
		s.LogVisualizerMessage(fmt.Sprintf("final update: Num previous light client updates: %d", len(previousPeriodLightClientUpdate)))
		// var previousLightClientUpdate ethereum.LightClientUpdateJSON
		for _, update := range previousPeriodLightClientUpdate {
			s.LogVisualizerMessage(fmt.Sprintf("final update: prev light client update slot: %d", update.Data.AttestedHeader.Beacon.Slot))
			s.LogVisualizerMessage(fmt.Sprintf("final update: prev light client update next sync c aggpubkey: %s", update.Data.NextSyncCommittee.AggregatePubkey))
			// if update.Data.NextSyncCommittee.AggregatePubkey == finalityUpdate.Data {
			// 	s.LogVisualizerMessage(fmt.Sprintf("final update: found previous light client update with same aggpubkey: %s", update.Data.NextSyncCommittee.AggregatePubkey))
			// 	previousLightClientUpdate = update
			// }
		}
		previousLightClientUpdate := previousPeriodLightClientUpdate[1]
		s.LogVisualizerMessage(fmt.Sprintf("final update: prev light client update slot: %d", previousLightClientUpdate.Data.AttestedHeader.Beacon.Slot))

		currentSyncCommitteePubkeys := [][]byte{}
		for _, pubkey := range previousLightClientUpdate.Data.NextSyncCommittee.Pubkeys {
			currentSyncCommitteePubkeys = append(currentSyncCommitteePubkeys, ethcommon.FromHex(pubkey))
		}

		consensusUpdate := finalityUpdate.ToLightClientUpdate()
		oldestHeader := ethereumligthclient.Header{
			ConsensusUpdate: &consensusUpdate,
			TrustedSyncCommittee: &ethereumligthclient.TrustedSyncCommittee{
				TrustedHeight: &clienttypes.Height{
					RevisionNumber: 0,
					RevisionHeight: unionConsensusState.Slot,
				},
				// NextSyncCommittee: &ethereumligthclient.SyncCommittee{
				// 	Pubkeys:         nextSyncCommitteePubkeys,
				// 	AggregatePubkey: ethcommon.FromHex(previousLightClientUpdate.Data.NextSyncCommittee.AggregatePubkey),
				// },
				CurrentSyncCommittee: &ethereumligthclient.SyncCommittee{
					Pubkeys:         currentSyncCommitteePubkeys,
					AggregatePubkey: ethcommon.FromHex(previousLightClientUpdate.Data.NextSyncCommittee.AggregatePubkey),
				},
			},
			AccountUpdate: &accountUpdate,
		}

		//		        let does_not_have_finality_update = last_update_block_number >= update_to.revision_height;
		doesNotHaveFinalityUpdate := lastUnionUpdate >= finalityUpdate.Data.AttestedHeader.Beacon.Slot
		var headers []ethereumligthclient.Header
		headers = append(headers, newHeaders...)

		if doesNotHaveFinalityUpdate {
			s.LogVisualizerMessage(fmt.Sprintf("does not have finality update: lastUpdateBlockNumber: %d, finalityUpdate slot: %d", lastUnionUpdate, finalityUpdate.Data.AttestedHeader.Beacon.Slot))
		} else {
			s.LogVisualizerMessage(fmt.Sprintf("has finality update: lastUpdateBlockNumber: %d, finalityUpdate slot: %d", lastUnionUpdate, finalityUpdate.Data.AttestedHeader.Beacon.Slot))
			headers = append(headers, oldestHeader)

		}

		s.LogVisualizerMessage(fmt.Sprintf("Num headers: %d", len(headers)))

		// #[error(
		//     "(update_signature_slot > update_attested_slot >= update_finalized_slot) must hold, \
		//     found: ({update_signature_slot} > {update_attested_slot} >= {update_finalized_slot})"
		// )]

		// current_slot >= update.signature_slot
		// && update.signature_slot > update_attested_slot
		// && update_attested_slot >= update_finalized_slot,

		wasmClientState, unionClientState := s.GetUnionClientState(ctx, s.unionClientID)
		_, unionConsensusState = s.GetUnionConsensusState(ctx, s.unionClientID, wasmClientState.LatestHeight)
		s.LogVisualizerMessage(fmt.Sprintf("submitting header to client with wasm latest height: %d", wasmClientState.LatestHeight.RevisionHeight))
		s.LogVisualizerMessage(fmt.Sprintf("submitting header to client with union latest height: %d", unionClientState.LatestSlot))
		s.LogVisualizerMessage(fmt.Sprintf("submitting header to client with union cons height: %d", unionConsensusState.Slot))
		s.LogVisualizerMessage(fmt.Sprintf("submitting header to client with union current cons pub agg key: %s", ethcommon.Bytes2Hex(unionConsensusState.CurrentSyncCommittee)))
		s.LogVisualizerMessage(fmt.Sprintf("submitting header to client with union next cons pub agg key: %s", ethcommon.Bytes2Hex(unionConsensusState.CurrentSyncCommittee)))

		s.LogVisualizerMessage("loop headers")
		for _, header := range headers {
			s.LogVisualizerMessage(fmt.Sprintf("submittiong header slot: %d", header.ConsensusUpdate.AttestedHeader.Beacon.Slot))
			s.LogVisualizerMessage(fmt.Sprintf("submitting header with trusted slot: %d", header.TrustedSyncCommittee.TrustedHeight.RevisionHeight))
			if header.TrustedSyncCommittee.CurrentSyncCommittee != nil {
				s.LogVisualizerMessage(fmt.Sprintf("submitting header with current sync committee: %s", ethcommon.Bytes2Hex(header.TrustedSyncCommittee.CurrentSyncCommittee.AggregatePubkey)))
			}
			if header.TrustedSyncCommittee.NextSyncCommittee != nil {
				s.LogVisualizerMessage(fmt.Sprintf("submitting header with next sync committee: %s", ethcommon.Bytes2Hex(header.TrustedSyncCommittee.NextSyncCommittee.AggregatePubkey)))
			}
			s.LogVisualizerMessage(fmt.Sprintf("submitting header with signature slot: %d", header.ConsensusUpdate.SignatureSlot))
			s.LogVisualizerMessage(fmt.Sprintf("submitting header with attested slot: %d", header.ConsensusUpdate.AttestedHeader.Beacon.Slot))
			s.LogVisualizerMessage(fmt.Sprintf("submitting header with finalized slot: %d", header.ConsensusUpdate.FinalizedHeader.Beacon.Slot))
			s.LogVisualizerMessage(fmt.Sprintf("submitting header with account update storage root: %s", ethcommon.Bytes2Hex(header.AccountUpdate.AccountProof.StorageRoot)))
			s.LogVisualizerMessage(fmt.Sprintf("submitting header with exec state root: %s", ethcommon.Bytes2Hex(header.ConsensusUpdate.AttestedHeader.Execution.StateRoot)))
		}

		for _, header := range headers {
			s.LogVisualizerMessage(fmt.Sprintf("submitting header slot: %d", header.ConsensusUpdate.AttestedHeader.Beacon.Slot))
			headerBz := simd.Config().EncodingConfig.Codec.MustMarshal(&header)
			wasmHeader := ibcwasmtypes.ClientMessage{
				Data: headerBz,
			}

			wasmHeaderAny, err := clienttypes.PackClientMessage(&wasmHeader)
			s.Require().NoError(err)
			_, err = s.BroadcastMessages(ctx, simd, simdRelayerUser, 200_000, &clienttypes.MsgUpdateClient{
				ClientId:      s.unionClientID,
				ClientMessage: wasmHeaderAny,
				Signer:        simdRelayerUser.FormattedAddress(),
			})
			s.Require().NoError(err)
			s.LogVisualizerMessage("OH MY FUCKING GOD, YES!!!!!")
			time.Sleep(10 * time.Second)

			if header.ConsensusUpdate.AttestedHeader.Beacon.Slot >= uint64(updateTo) {
				s.LogVisualizerMessage("we have updated past updateTo! we should be able to prove now!")
				break
			}
		}
	}))

	// s.Require().True(s.Run("Update client on Cosmos chain", func() {
	// 	wasmClientState, unionClientState := s.GetUnionClientState(ctx, s.unionClientID)
	// 	_, unionConsensusState := s.GetUnionConsensusState(ctx, s.unionClientID, wasmClientState.LatestHeight)
	//
	// 	spec, err := eth.BeaconAPIClient.GetSpec()
	// 	s.Require().NoError(err)
	//
	// 	time.Sleep(5 * time.Second)
	// 	trustedPeriod := wasmClientState.LatestHeight.RevisionHeight / spec.Period()
	//
	// 	targetPeriod := trustedPeriod
	// 	err = testutil.WaitForCondition(5*time.Minute, 5*time.Second, func() (bool, error) {
	// 		s.LogVisualizerMessage("Waiting for finalized target period to be greater than trusted period")
	// 		finalityUpdate, err := eth.BeaconAPIClient.GetFinalityUpdate()
	// 		if err != nil {
	// 			return false, err
	// 		}
	//
	// 		targetPeriod = finalityUpdate.Data.AttestedHeader.Beacon.Slot / spec.Period()
	//
	// 		return targetPeriod > trustedPeriod, nil
	// 	})
	// 	s.Require().NoError(err)
	//
	// 	s.LogVisualizerMessage(fmt.Sprintf("targetPeriod: %d", targetPeriod))
	// 	s.LogVisualizerMessage(fmt.Sprintf("trustedPeriod: %d", trustedPeriod))
	// 	s.Require().Greater(targetPeriod, trustedPeriod)
	//
	// 	lightClientUpdates, err := eth.BeaconAPIClient.GetLightClientUpdates(trustedPeriod+1, targetPeriod-trustedPeriod)
	// 	s.Require().NoError(err)
	//
	// 	// var lightClientUpdates ethereum.LightClientUpdatesResponse
	// 	// err = testutil.WaitForCondition(5*time.Minute, 5*time.Second, func() (bool, error) {
	// 	// 	s.LogVisualizerMessage("Waiting for light client updates")
	// 	// 	lightClientUpdates, err = eth.BeaconAPIClient.GetLightClientUpdates(trustedPeriod+1, targetPeriod-trustedPeriod)
	// 	// 	if err != nil {
	// 	// 		return false, err
	// 	// 	}
	// 	//
	// 	// 	return len(lightClientUpdates) > 0, nil
	// 	// })
	// 	// s.Require().NoError(err)
	// 	//
	// 	s.LogVisualizerMessage(fmt.Sprintf("Num light client updates: %d", len(lightClientUpdates)))
	//
	// 	lightClientUpdate := lightClientUpdates[0]
	// 	s.LogVisualizerMessage(fmt.Sprintf("light client update slot: %d", lightClientUpdate.Data.AttestedHeader.Beacon.Slot))
	//
	// 	executionHeight, err := eth.BeaconAPIClient.GetExecutionHeight(fmt.Sprintf("%d", lightClientUpdate.Data.AttestedHeader.Beacon.Slot))
	// 	s.Require().NoError(err)
	// 	executionHeightHex := fmt.Sprintf("0x%x", executionHeight)
	// 	s.LogVisualizerMessage(fmt.Sprintf("Execution height: %s", executionHeightHex))
	// 	proofResp, err := eth.EthAPI.GetProof(s.contractAddresses.Ics26Router, []string{}, executionHeightHex)
	// 	s.Require().NoError(err)
	// 	s.Require().NotEmpty(proofResp.AccountProof)
	//
	// 	testHeader, err := eth.BeaconAPIClient.GetHeader(strconv.Itoa(int(executionHeight)))
	// 	s.Require().NoError(err)
	//
	// 	var testBootstrap ethereum.Bootstrap
	// 	err = testutil.WaitForCondition(5*time.Minute, 5*time.Second, func() (bool, error) {
	// 		testBootstrap, err = eth.BeaconAPIClient.GetBootstrap(testHeader.Root)
	// 		if err != nil {
	// 			return false, nil
	// 		}
	//
	// 		return true, nil
	// 	})
	// 	s.Require().NoError(err)
	// 	s.LogVisualizerMessage(fmt.Sprintf("exec height bootstrap sync committee aggpubkey: %s", testBootstrap.Data.CurrentSyncCommittee.AggregatePubkey))
	// 	s.Require().Equal(ethcommon.FromHex(testBootstrap.Data.CurrentSyncCommittee.AggregatePubkey), unionConsensusState.NextSyncCommittee)
	//
	// 	var proofBz [][]byte
	// 	for _, proofStr := range proofResp.AccountProof {
	// 		proofBz = append(proofBz, ethcommon.FromHex(proofStr))
	// 	}
	// 	accountUpdate := ethereumligthclient.AccountUpdate{
	// 		AccountProof: &ethereumligthclient.AccountProof{
	// 			StorageRoot: ethereum.HexToBeBytes(proofResp.StorageHash),
	// 			Proof:       proofBz,
	// 		},
	// 	}
	//
	// 	var pubkeys [][]byte
	// 	for _, pubkey := range s.initialNextSyncComittee.Pubkeys {
	// 		pubkeys = append(pubkeys, ethcommon.FromHex(pubkey))
	// 	}
	//
	// 	// currentSyncCommittee := ethereumligthclient.SyncCommittee{
	// 	// 	Pubkeys:         pubkeys,
	// 	// 	AggregatePubkey: ethcommon.FromHex(lightClientUpdate.Data.NextSyncCommittee.AggregatePubkey),
	// 	// }
	// 	// _ = currentSyncCommittee
	// 	nextSyncCommittee := ethereumligthclient.SyncCommittee{
	// 		Pubkeys:         pubkeys,
	// 		AggregatePubkey: unionConsensusState.NextSyncCommittee,
	// 	}
	// 	s.LogVisualizerMessage(fmt.Sprintf("lightclientupdate sync committee: %s", lightClientUpdate.Data.NextSyncCommittee.AggregatePubkey))
	// 	s.LogVisualizerMessage(fmt.Sprintf("current consensus state sync committee aggpubkey: %s", ethcommon.Bytes2Hex(unionConsensusState.CurrentSyncCommittee)))
	// 	s.LogVisualizerMessage(fmt.Sprintf("current consensus state next sync committee aggpubkey: %s", ethcommon.Bytes2Hex(unionConsensusState.NextSyncCommittee)))
	//
	// 	trustedSyncCommittee := ethereumligthclient.TrustedSyncCommittee{
	// 		TrustedHeight: &wasmClientState.LatestHeight,
	// 		// CurrentSyncCommittee: &currentSyncCommittee,
	// 		NextSyncCommittee: &nextSyncCommittee,
	// 	}
	//
	// 	consensusLightClientUpdate := lightClientUpdate.ToLightClientUpdate()
	// 	header := ethereumligthclient.Header{
	// 		// ConsensusUpdate: &lightClientUpdate,
	// 		ConsensusUpdate:      &consensusLightClientUpdate,
	// 		TrustedSyncCommittee: &trustedSyncCommittee,
	// 		AccountUpdate:        &accountUpdate,
	// 	}
	//
	// 	s.LogVisualizerMessage(fmt.Sprintf("Union client state: %+v", unionClientState))
	// 	s.LogVisualizerMessage(fmt.Sprintf("Union consensus state: %+v", unionConsensusState))
	//
	// 	currentSlotCalculated := (uint64(time.Now().Unix()) - unionClientState.GenesisTime) / uint64(spec.SecondsPerSlot.Seconds())
	// 	s.Require().GreaterOrEqual(currentSlotCalculated, consensusLightClientUpdate.SignatureSlot)
	// 	s.Require().Greater(consensusLightClientUpdate.SignatureSlot, consensusLightClientUpdate.AttestedHeader.Beacon.Slot)
	// 	s.Require().GreaterOrEqual(consensusLightClientUpdate.AttestedHeader.Beacon.Slot, consensusLightClientUpdate.FinalizedHeader.Beacon.Slot)
	//
	// 	var proofBzJSON []string
	// 	for _, proof := range proofBz {
	// 		proofBzJSON = append(proofBzJSON, ethcommon.Bytes2Hex(proof))
	// 	}
	//
	// 	headerBz := simd.Config().EncodingConfig.Codec.MustMarshal(&header)
	// 	wasmHeader := ibcwasmtypes.ClientMessage{
	// 		Data: headerBz,
	// 	}
	//
	// 	wasmHeaderAny, err := clienttypes.PackClientMessage(&wasmHeader)
	// 	s.Require().NoError(err)
	// 	_, err = s.BroadcastMessages(ctx, simd, simdRelayerUser, 200_000, &clienttypes.MsgUpdateClient{
	// 		ClientId:      s.unionClientID,
	// 		ClientMessage: wasmHeaderAny,
	// 		Signer:        simdRelayerUser.FormattedAddress(),
	// 	})
	// 	s.Require().NoError(err)
	// }))

	var recvAck []byte
	var denomOnCosmos transfertypes.DenomTrace
	s.Require().True(s.Run("recvPacket on Cosmos chain", func() {
		proofHeight := clienttypes.Height{
			RevisionNumber: 0,
			RevisionHeight: lastUnionUpdate,
		}
		_, unionConsState := s.GetUnionConsensusState(ctx, s.unionClientID, proofHeight)
		s.LogVisualizerMessage(fmt.Sprintf("recv: trusted slot (union cons slot): %d", unionConsState.Slot))
		s.LogVisualizerMessage(fmt.Sprintf("recv: state root: %s", ethcommon.Bytes2Hex(unionConsState.StateRoot)))
		s.LogVisualizerMessage(fmt.Sprintf("recv: state root: %s", ethcommon.Bytes2Hex(unionConsState.StorageRoot)))

		path := fmt.Sprintf("commitments/ports/%s/channels/%s/sequences/%d", sendPacket.SourcePort, sendPacket.SourceChannel, sendPacket.Sequence)
		s.LogVisualizerMessage(fmt.Sprintf("recv: path: %s", path))
		storageKey := ethereum.GetStorageKey(path)
		s.LogVisualizerMessage(fmt.Sprintf("recv: storage key: %s", storageKey.Hex()))
		storageKeys := []string{storageKey.Hex()}

		blockNumberHex := fmt.Sprintf("0x%x", lastUnionUpdate)
		s.LogVisualizerMessage(fmt.Sprintf("recv: proof block number: %s", blockNumberHex))
		s.LogVisualizerMessage(fmt.Sprintf("recv: ics26Router: %s", s.contractAddresses.Ics26Router))
		proofResp, err := eth.EthAPI.GetProof(s.contractAddresses.Ics26Router, storageKeys, blockNumberHex)
		s.Require().NoError(err)
		s.Require().Len(proofResp.StorageProof, 1)

		var proofBz [][]byte
		for _, proofStr := range proofResp.StorageProof[0].Proof {
			proofBz = append(proofBz, ethcommon.FromHex(proofStr))
		}
		storageProof := ethereumligthclient.StorageProof{
			Key:   ethereum.HexToBeBytes(proofResp.StorageProof[0].Key),
			Value: ethereum.HexToBeBytes(proofResp.StorageProof[0].Value),
			Proof: proofBz,
		}
		s.LogVisualizerMessage(fmt.Sprintf("StorageProof Key: %s, Value: %s, Proof: %+v", proofResp.StorageProof[0].Key, proofResp.StorageProof[0].Value, proofResp.StorageProof[0].Proof))
		storageProofBz := simd.Config().EncodingConfig.Codec.MustMarshal(&storageProof)

		s.LogVisualizerMessage(fmt.Sprintf("recv: path from contract event: %s", ethcommon.Bytes2Hex(commitEvent.Path[:])))
		s.LogVisualizerMessage(fmt.Sprintf("recv: commitment from contract event: %s", ethcommon.Bytes2Hex(commitEvent.Commitment[:])))

		packet := channeltypes.Packet{
			Sequence:           uint64(sendPacket.Sequence),
			SourcePort:         sendPacket.SourcePort,
			SourceChannel:      sendPacket.SourceChannel,
			DestinationPort:    sendPacket.DestPort,
			DestinationChannel: sendPacket.DestChannel,
			Data:               sendPacket.Data,
			TimeoutHeight:      clienttypes.Height{},
			TimeoutTimestamp:   sendPacket.TimeoutTimestamp * 1_000_000_000,
		}

		s.LogVisualizerMessage(fmt.Sprintf(`recv: packet sent in:
timeout: %d
dest port: %s
dest channel: %s
data (unhashed, and as hex): %s
`, packet.TimeoutTimestamp, packet.DestinationPort, packet.DestinationChannel, hex.EncodeToString(sendPacket.Data)))

		goCommitment := channeltypes.CommitLitePacket(simd.GetCodec(), packet)
		s.LogVisualizerMessage(fmt.Sprintf("recv: goCommitment: %s", hex.EncodeToString(goCommitment)))

		txResp, err := s.BroadcastMessages(ctx, simd, cosmosUserWallet, 200_000, &channeltypes.MsgRecvPacket{
			Packet:          packet,
			ProofCommitment: storageProofBz,
			ProofHeight:     proofHeight,
			Signer:          cosmosUserAddress,
		})
		s.Require().NoError(err)

		recvAck, err = ibctesting.ParseAckFromEvents(txResp.Events)
		s.Require().NoError(err)
		s.Require().NotNil(recvAck)

		s.Require().True(s.Run("Verify balances on Cosmos chain", func() {
			denomOnCosmos = transfertypes.ParseDenomTrace(
				fmt.Sprintf("%s/%s/%s", transfertypes.PortID, "08-wasm-0", s.contractAddresses.Erc20),
			)

			// User balance on Cosmos chain
			resp, err := e2esuite.GRPCQuery[banktypes.QueryBalanceResponse](ctx, simd, &banktypes.QueryBalanceRequest{
				Address: cosmosUserAddress,
				Denom:   denomOnCosmos.IBCDenom(),
			})
			s.Require().NoError(err)
			s.Require().NotNil(resp.Balance)
			s.Require().Equal(sdkmath.NewIntFromBigInt(transferAmount), resp.Balance.Amount)
			s.Require().Equal(denomOnCosmos.IBCDenom(), resp.Balance.Denom)
		}))
	}))

	s.Require().True(s.Run("acknowledgePacket on Ethereum", func() {
		clientState, err := s.sp1Ics07Contract.GetClientState(nil)
		s.Require().NoError(err)

		trustedHeight := clientState.LatestHeight.RevisionHeight
		latestHeight, err := simd.Height(ctx)
		s.Require().NoError(err)

		// This will be a membership proof since the acknowledgement is written
		packetAckPath := ibchost.PacketAcknowledgementPath(sendPacket.DestPort, sendPacket.DestChannel, uint64(sendPacket.Sequence))
		proofHeight, ucAndMemProof, err := operator.UpdateClientAndMembershipProof(
			uint64(trustedHeight), uint64(latestHeight), packetAckPath,
			"--trust-level", testvalues.DefaultTrustLevel.String(),
			"--trusting-period", strconv.Itoa(testvalues.DefaultTrustPeriod),
		)
		s.Require().NoError(err)

		msg := ics26router.IICS26RouterMsgsMsgAckPacket{
			Packet:          sendPacket,
			Acknowledgement: recvAck,
			ProofAcked:      ucAndMemProof,
			ProofHeight:     *proofHeight,
		}

		fmt.Printf("ack packet: %+v\n", msg)

		tx, err := s.ics26Contract.AckPacket(s.GetTransactOpts(s.key), msg)
		s.Require().NoError(err)

		receipt := s.GetTxReciept(ctx, eth, tx.Hash())
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)

		if s.generateFixtures {
			s.Require().NoError(types.GenerateAndSaveFixture("acknowledgePacket.json", s.contractAddresses.Erc20, "ackPacket", msg, sendPacket))
		}

		s.Require().True(s.Run("Verify balances on Ethereum", func() {
			// User balance on Ethereum
			userBalance, err := s.erc20Contract.BalanceOf(nil, ethereumUserAddress)
			s.Require().NoError(err)
			s.Require().Equal(testvalues.InitialBalance-testvalues.TransferAmount, userBalance.Int64())

			// ICS20 contract balance on Ethereum
			// ICS20 contract balance on Ethereum
			escrowBalance, err := s.erc20Contract.BalanceOf(nil, s.escrowContractAddr)
			s.Require().NoError(err)
			s.Require().Equal(testvalues.TransferAmount, escrowBalance.Int64())
		}))
	}))

	var returnPacket channeltypes.Packet
	s.Require().True(s.Run("Transfer tokens back from Cosmos chain", func() {
		// We need the timeout to be a whole number of seconds to be received by eth
		timeout := uint64(time.Now().Add(30*time.Minute).Unix() * 1_000_000_000)
		ibcCoin := sdk.NewCoin(denomOnCosmos.IBCDenom(), sdkmath.NewIntFromBigInt(transferAmount))

		msgTransfer := transfertypes.MsgTransfer{
			SourcePort:       transfertypes.PortID,
			SourceChannel:    s.unionClientID,
			Token:            ibcCoin,
			Sender:           cosmosUserAddress,
			Receiver:         strings.ToLower(ethereumUserAddress.Hex()),
			TimeoutHeight:    clienttypes.Height{},
			TimeoutTimestamp: timeout,
			Memo:             "",
			DestPort:         transfertypes.PortID,
			DestChannel:      s.tendermintClientID,
		}

		txResp, err := s.BroadcastMessages(ctx, simd, cosmosUserWallet, 200_000, &msgTransfer)
		s.Require().NoError(err)
		returnPacket, err = ibctesting.ParsePacketFromEvents(txResp.Events)
		s.Require().NoError(err)

		s.Require().Equal(uint64(1), returnPacket.Sequence)
		s.Require().Equal(transfertypes.PortID, returnPacket.SourcePort)
		s.Require().Equal(s.unionClientID, returnPacket.SourceChannel)
		s.Require().Equal(transfertypes.PortID, returnPacket.DestinationPort)
		s.Require().Equal(s.tendermintClientID, returnPacket.DestinationChannel)
		s.Require().Equal(clienttypes.Height{}, returnPacket.TimeoutHeight)
		s.Require().Equal(timeout, returnPacket.TimeoutTimestamp)

		var transferPacketData transfertypes.FungibleTokenPacketData
		err = json.Unmarshal(returnPacket.Data, &transferPacketData)
		s.Require().NoError(err)
		s.Require().Equal(denomOnCosmos.GetFullDenomPath(), transferPacketData.Denom)
		s.Require().Equal(transferAmount.String(), transferPacketData.Amount)
		s.Require().Equal(cosmosUserAddress, transferPacketData.Sender)
		s.Require().Equal(strings.ToLower(ethereumUserAddress.Hex()), transferPacketData.Receiver)
		s.Require().Equal("", transferPacketData.Memo)

		s.Require().True(s.Run("Verify balances on Cosmos chain", func() {
			// User balance on Cosmos chain
			resp, err := e2esuite.GRPCQuery[banktypes.QueryBalanceResponse](ctx, simd, &banktypes.QueryBalanceRequest{
				Address: cosmosUserAddress,
				Denom:   denomOnCosmos.IBCDenom(),
			})
			s.Require().NoError(err)
			s.Require().NotNil(resp.Balance)
			s.Require().Equal(sdkmath.ZeroInt(), resp.Balance.Amount)
			s.Require().Equal(denomOnCosmos.GetFullDenomPath(), resp.Balance.Denom)
		}))
	}))

	var returnWriteAckEvent *ics26router.ContractWriteAcknowledgement
	s.Require().True(s.Run("Receive packet on Ethereum", func() {
		clientState, err := s.sp1Ics07Contract.GetClientState(nil)
		s.Require().NoError(err)

		trustedHeight := clientState.LatestHeight.RevisionHeight
		latestHeight, err := simd.Height(ctx)
		s.Require().NoError(err)

		packetCommitmentPath := ibchost.PacketCommitmentPath(returnPacket.SourcePort, returnPacket.SourceChannel, returnPacket.Sequence)
		proofHeight, ucAndMemProof, err := operator.UpdateClientAndMembershipProof(
			uint64(trustedHeight), uint64(latestHeight), packetCommitmentPath,
			"--trust-level", testvalues.DefaultTrustLevel.String(),
			"--trusting-period", strconv.Itoa(testvalues.DefaultTrustPeriod),
		)
		s.Require().NoError(err)

		packet := ics26router.IICS26RouterMsgsPacket{
			Sequence:         uint32(returnPacket.Sequence),
			TimeoutTimestamp: returnPacket.TimeoutTimestamp / 1_000_000_000,
			SourcePort:       returnPacket.SourcePort,
			SourceChannel:    returnPacket.SourceChannel,
			DestPort:         returnPacket.DestinationPort,
			DestChannel:      returnPacket.DestinationChannel,
			Version:          transfertypes.Version,
			Data:             returnPacket.Data,
		}
		msg := ics26router.IICS26RouterMsgsMsgRecvPacket{
			Packet:          packet,
			ProofCommitment: ucAndMemProof,
			ProofHeight:     *proofHeight,
		}

		tx, err := s.ics26Contract.RecvPacket(s.GetTransactOpts(s.key), msg)
		s.Require().NoError(err)

		receipt := s.GetTxReciept(ctx, eth, tx.Hash())
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)

		if s.generateFixtures {
			s.Require().NoError(types.GenerateAndSaveFixture("receivePacket.json", s.contractAddresses.Erc20, "recvPacket", msg, packet))
		}

		returnWriteAckEvent, err = e2esuite.GetEvmEvent(receipt, s.ics26Contract.ParseWriteAcknowledgement)
		s.Require().NoError(err)

		receiveEvent, err := e2esuite.GetEvmEvent(receipt, s.ics20Contract.ParseICS20ReceiveTransfer)
		s.Require().NoError(err)
		ethReceiveData := receiveEvent.PacketData
		s.Require().Equal(denomOnCosmos.GetFullDenomPath(), ethReceiveData.Denom)
		s.Require().Equal(s.contractAddresses.Erc20, strings.ToLower(receiveEvent.Erc20Address.Hex()))
		s.Require().Equal(cosmosUserAddress, ethReceiveData.Sender)
		s.Require().Equal(strings.ToLower(ethereumUserAddress.Hex()), ethReceiveData.Receiver)
		s.Require().Equal(transferAmount, ethReceiveData.Amount) // the amount transferred the user on the evm side is converted, but the packet doesn't change
		s.Require().Equal("", ethReceiveData.Memo)

		s.True(s.Run("Verify balances on Ethereum", func() {
			// User balance should be back to the starting point
			userBalance, err := s.erc20Contract.BalanceOf(nil, ethereumUserAddress)
			s.Require().NoError(err)
			s.Require().Equal(testvalues.InitialBalance, userBalance.Int64())

			escrowBalance, err := s.erc20Contract.BalanceOf(nil, s.escrowContractAddr)
			s.Require().NoError(err)
			s.Require().Equal(int64(0), escrowBalance.Int64())
		}))
	}))

	// TODO: When using a non-mock light client on the cosmos chain, the client there needs to be updated at this point

	s.Require().True(s.Run("Acknowledge packet on Cosmos chain", func() {
		wasmClientState, _ := s.GetUnionClientState(ctx, s.unionClientID)

		// TODO: Create proof for the ack

		txResp, err := s.BroadcastMessages(ctx, simd, cosmosUserWallet, 200_000, &channeltypes.MsgAcknowledgement{
			Packet:          returnPacket,
			Acknowledgement: returnWriteAckEvent.Acknowledgement,
			ProofAcked:      []byte("doesn't matter"), // Because mock light client
			ProofHeight:     wasmClientState.LatestHeight,
			Signer:          cosmosUserAddress,
		})
		s.Require().NoError(err)
		s.Require().Equal(uint32(0), txResp.Code)
	}))
}

func TestFastCommitment(t *testing.T) {
	packet := channeltypes.Packet{
		Sequence:           1,
		Data:               ethcommon.FromHex("7b2264656e6f6d223a22307831643234666361326633633831633761633438613937303533313534636164353166373137643335222c22616d6f756e74223a2231303030303030303030222c2273656e646572223a22307866653566333561396638383831353535346434613965363932396239316635666537623936393964222c227265636569766572223a22636f736d6f7331646135396d7776686479753577746d36356e776533776678706d676733796333386d716b3732222c226d656d6f223a22227d"),
		TimeoutHeight:      clienttypes.Height{},
		TimeoutTimestamp:   1729659372000000000,
		DestinationPort:    "transfer",
		DestinationChannel: "08-wasm-0",
	}

	fmt.Printf(`recv: packet sent in:
timeout: %d
data (unhashed, and as hex): %s
`, packet.TimeoutTimestamp, hex.EncodeToString(packet.Data))

	goCommitment := channeltypes.CommitLitePacket(chainconfig.CosmosEncodingConfig().Codec, packet)
	beGoCommitment := ethereum.HexToBeBytes(ethcommon.Bytes2Hex(goCommitment))
	fmt.Printf("goComm bytes len: %d\n", len(goCommitment))
	fmt.Printf("beGoComm bytes len: %d\n", len(beGoCommitment))
	fmt.Printf("recv: goCommitment: %s\n", hex.EncodeToString(goCommitment))
	fmt.Println("from test", "0xb9495b2e5458dd790ca6d91efd83a815618f24d29c3fda341a5e8bf0582785f7")

	require.Equal(t, "3ec9c8f927b36ae8bdd1431e5f91fabc34667adbaf9fb1334353809ad465e063", hex.EncodeToString(goCommitment))
}

func TestFastProof(t *testing.T) {
	ethAPI, err := ethereum.NewEthAPI("http://localhost:32857")
	require.NoError(t, err)

	blockNumberHex, _, err := ethAPI.GetBlockNumber()
	require.NoError(t, err)

	path := "commitments/ports/transfer/channels/07-tendermint-0/sequences/1"

	fmt.Printf("recv: path: %s\n", path)
	storageKey := ethereum.GetStorageKey(path)
	fmt.Printf("recv: storage key: %s\n", storageKey.Hex())
	storageKeys := []string{storageKey.Hex()}

	blockNumberHex = fmt.Sprintf("0x%x", 81)
	fmt.Printf("recv: proof block number: %s\n", blockNumberHex)
	ics26Router := "0xcd906bb056d0c65b198154d576de6aac349b6bdf"
	fmt.Printf("recv: ics26Router: %s\n", ics26Router)

	proofResp, err := ethAPI.GetProof(ics26Router, storageKeys, blockNumberHex)
	require.NoError(t, err)
	require.Len(t, proofResp.StorageProof, 1)

	var proofBz [][]byte
	for _, proofStr := range proofResp.StorageProof[0].Proof {
		proofBz = append(proofBz, ethcommon.FromHex(proofStr))
	}
	storageProof := ethereumligthclient.StorageProof{
		Key:   ethereum.HexToBeBytes(proofResp.StorageProof[0].Key),
		Value: ethereum.HexToBeBytes(proofResp.StorageProof[0].Value),
		Proof: proofBz,
	}
	fmt.Printf("StorageProof Key: %s, Value: %s, Proof: %+v\n", proofResp.StorageProof[0].Key, proofResp.StorageProof[0].Value, proofResp.StorageProof[0].Proof)
	_ = storageProof

}

// FundAddressChainB sends funds to the given address on Chain B.
// The amount sent is 1,000,000,000 of the chain's denom.
func (s *FastSuite) FundAddressChainB(ctx context.Context, address string) {
	s.fundAddress(ctx, s.ChainB, s.UserB.KeyName(), address)
}

// BroadcastMessages broadcasts the provided messages to the given chain and signs them on behalf of the provided user.
// Once the broadcast response is returned, we wait for two blocks to be created on chain.
func (s *FastSuite) BroadcastMessages(ctx context.Context, chain *cosmos.CosmosChain, user ibc.Wallet, gas uint64, msgs ...sdk.Msg) (*sdk.TxResponse, error) {
	sdk.GetConfig().SetBech32PrefixForAccount(chain.Config().Bech32Prefix, chain.Config().Bech32Prefix+sdk.PrefixPublic)
	sdk.GetConfig().SetBech32PrefixForValidator(
		chain.Config().Bech32Prefix+sdk.PrefixValidator+sdk.PrefixOperator,
		chain.Config().Bech32Prefix+sdk.PrefixValidator+sdk.PrefixOperator+sdk.PrefixPublic,
	)

	broadcaster := cosmos.NewBroadcaster(s.T(), chain)

	broadcaster.ConfigureClientContextOptions(func(clientContext client.Context) client.Context {
		return clientContext.
			WithCodec(chain.Config().EncodingConfig.Codec).
			WithChainID(chain.Config().ChainID).
			WithTxConfig(chain.Config().EncodingConfig.TxConfig)
	})

	broadcaster.ConfigureFactoryOptions(func(factory tx.Factory) tx.Factory {
		return factory.WithGas(gas)
	})

	resp, err := cosmos.BroadcastTx(ctx, broadcaster, user, msgs...)
	if err != nil {
		return nil, err
	}

	// wait for 2 blocks for the transaction to be included
	s.Require().NoError(testutil.WaitForBlocks(ctx, 2, chain))

	return &resp, nil
}

// fundAddress sends funds to the given address on the given chain
func (s *FastSuite) fundAddress(ctx context.Context, chain *cosmos.CosmosChain, keyName, address string) {
	err := chain.SendFunds(ctx, keyName, ibc.WalletAmount{
		Address: address,
		Denom:   chain.Config().Denom,
		Amount:  sdkmath.NewInt(1_000_000_000),
	})
	s.Require().NoError(err)

	// wait for 2 blocks for the funds to be received
	err = testutil.WaitForBlocks(ctx, 2, chain)
	s.Require().NoError(err)
}

// GetRelayerUsers returns two ibc.Wallet instances which can be used for the relayer users
// on the two chains.
func (s *FastSuite) GetRelayerUsers(ctx context.Context) (*ecdsa.PrivateKey, ibc.Wallet) {
	eth, simd := s.ChainA, s.ChainB

	ethKey, err := eth.CreateAndFundUser()
	s.Require().NoError(err)

	cosmosUserFunds := sdkmath.NewInt(testvalues.InitialBalance)
	cosmosUsers := interchaintest.GetAndFundTestUsers(s.T(), ctx, s.T().Name(), cosmosUserFunds, simd)

	return ethKey, cosmosUsers[0]
}

// GetEvmEvent parses the logs in the given receipt and returns the first event that can be parsed
func GetEvmEvent[T any](receipt *ethtypes.Receipt, parseFn func(log ethtypes.Log) (*T, error)) (event *T, err error) {
	for _, l := range receipt.Logs {
		event, err = parseFn(*l)
		if err == nil && event != nil {
			break
		}
	}

	if event == nil {
		err = fmt.Errorf("event not found")
	}

	return
}

func (s *FastSuite) GetTxReciept(ctx context.Context, chain ethereum.Ethereum, hash ethcommon.Hash) *ethtypes.Receipt {
	ethClient, err := ethclient.Dial(chain.RPC)
	s.Require().NoError(err)

	var receipt *ethtypes.Receipt
	err = testutil.WaitForCondition(time.Second*30, time.Second, func() (bool, error) {
		receipt, err = ethClient.TransactionReceipt(ctx, hash)
		if err != nil {
			return false, nil
		}

		return receipt != nil, nil
	})
	s.Require().NoError(err)
	return receipt
}

func (s *FastSuite) GetTransactOpts(key *ecdsa.PrivateKey) *bind.TransactOpts {
	txOpts, err := bind.NewKeyedTransactorWithChainID(key, s.ChainA.ChainID)
	s.Require().NoError(err)

	return txOpts
}

// PushNewWasmClientProposal submits a new wasm client governance proposal to the chain.
func (s *FastSuite) PushNewWasmClientProposal(ctx context.Context, chain *cosmos.CosmosChain, wallet ibc.Wallet, proposalContentReader io.Reader) string {
	zippedContent, err := io.ReadAll(proposalContentReader)
	s.Require().NoError(err)

	computedChecksum := s.extractChecksumFromGzippedContent(zippedContent)

	s.Require().NoError(err)
	message := ibcwasmtypes.MsgStoreCode{
		Signer:       authtypes.NewModuleAddress(govtypes.ModuleName).String(),
		WasmByteCode: zippedContent,
	}

	err = s.ExecuteGovV1Proposal(ctx, &message, chain, wallet)
	s.Require().NoError(err)

	codeResp, err := e2esuite.GRPCQuery[ibcwasmtypes.QueryCodeResponse](ctx, chain, &ibcwasmtypes.QueryCodeRequest{Checksum: computedChecksum})
	s.Require().NoError(err)

	checksumBz := codeResp.Data
	checksum32 := sha256.Sum256(checksumBz)
	actualChecksum := hex.EncodeToString(checksum32[:])
	s.Require().Equal(computedChecksum, actualChecksum, "checksum returned from query did not match the computed checksum")

	return actualChecksum
}

// extractChecksumFromGzippedContent takes a gzipped wasm contract and returns the checksum.
func (s *FastSuite) extractChecksumFromGzippedContent(zippedContent []byte) string {
	content, err := ibcwasmtypes.Uncompress(zippedContent, ibcwasmtypes.MaxWasmSize)
	s.Require().NoError(err)

	checksum32 := sha256.Sum256(content)
	return hex.EncodeToString(checksum32[:])
}

// ExecuteGovV1Proposal submits a v1 governance proposal using the provided user and message and uses all validators
// to vote yes on the proposal.
func (s *FastSuite) ExecuteGovV1Proposal(ctx context.Context, msg sdk.Msg, cosmosChain *cosmos.CosmosChain, user ibc.Wallet) error {
	sender, err := sdk.AccAddressFromBech32(user.FormattedAddress())
	s.Require().NoError(err)

	proposalID := s.proposalIDs[cosmosChain.Config().ChainID]
	defer func() {
		s.proposalIDs[cosmosChain.Config().ChainID] = proposalID + 1
	}()

	msgs := []sdk.Msg{msg}

	msgSubmitProposal, err := govtypesv1.NewMsgSubmitProposal(
		msgs,
		sdk.NewCoins(sdk.NewCoin(cosmosChain.Config().Denom, govtypesv1.DefaultMinDepositTokens)),
		sender.String(),
		"",
		fmt.Sprintf("e2e gov proposal: %d", proposalID),
		fmt.Sprintf("executing gov proposal %d", proposalID),
		false,
	)
	s.Require().NoError(err)

	_, err = s.BroadcastMessages(ctx, cosmosChain, user, 50_000_000, msgSubmitProposal)
	s.Require().NoError(err)

	s.Require().NoError(cosmosChain.VoteOnProposalAllValidators(ctx, strconv.Itoa(int(proposalID)), cosmos.ProposalVoteYes))

	return s.waitForGovV1ProposalToPass(ctx, cosmosChain, proposalID)
}

// waitForGovV1ProposalToPass polls for the entire voting period to see if the proposal has passed.
// if the proposal has not passed within the duration of the voting period, an error is returned.
func (*FastSuite) waitForGovV1ProposalToPass(ctx context.Context, chain *cosmos.CosmosChain, proposalID uint64) error {
	var govProposal *govtypesv1.Proposal
	// poll for the query for the entire voting period to see if the proposal has passed.
	err := testutil.WaitForCondition(testvalues.VotingPeriod, 10*time.Second, func() (bool, error) {
		proposalResp, err := e2esuite.GRPCQuery[govtypesv1.QueryProposalResponse](ctx, chain, &govtypesv1.QueryProposalRequest{
			ProposalId: proposalID,
		})
		if err != nil {
			return false, err
		}

		govProposal = proposalResp.Proposal
		return govProposal.Status == govtypesv1.StatusPassed, nil
	})

	// in the case of a failed proposal, we wrap the polling error with additional information about why the proposal failed.
	if err != nil && govProposal.FailedReason != "" {
		err = errorsmod.Wrap(err, govProposal.FailedReason)
	}
	return err
}

func IsLowercase(s string) bool {
	for _, r := range s {
		if !unicode.IsLower(r) && unicode.IsLetter(r) {
			return false
		}
	}
	return true
}

func (s *FastSuite) GetUnionClientState(ctx context.Context, clientID string) (*ibcwasmtypes.ClientState, ethereumligthclient.ClientState) {
	simd := s.ChainB
	clientStateResp, err := e2esuite.GRPCQuery[clienttypes.QueryClientStateResponse](ctx, simd, &clienttypes.QueryClientStateRequest{
		ClientId: clientID,
	})
	s.Require().NoError(err)

	var clientState ibcexported.ClientState
	err = simd.Config().EncodingConfig.InterfaceRegistry.UnpackAny(clientStateResp.ClientState, &clientState)
	s.Require().NoError(err)

	wasmClientState, ok := clientState.(*ibcwasmtypes.ClientState)
	s.Require().True(ok)
	s.Require().NotEmpty(wasmClientState.Data)

	var ethClientState ethereumligthclient.ClientState
	err = simd.Config().EncodingConfig.Codec.Unmarshal(wasmClientState.Data, &ethClientState)
	s.Require().NoError(err)

	return wasmClientState, ethClientState
}

func (s *FastSuite) GetUnionConsensusState(ctx context.Context, clientID string, height clienttypes.Height) (*ibcwasmtypes.ConsensusState, ethereumligthclient.ConsensusState) {
	simd := s.ChainB
	consensusStateResp, err := e2esuite.GRPCQuery[clienttypes.QueryConsensusStateResponse](ctx, simd, &clienttypes.QueryConsensusStateRequest{
		ClientId:       clientID,
		RevisionNumber: height.RevisionNumber,
		RevisionHeight: height.RevisionHeight,
		LatestHeight:   false,
	})
	s.Require().NoError(err)

	var consensusState ibcexported.ConsensusState
	err = simd.Config().EncodingConfig.InterfaceRegistry.UnpackAny(consensusStateResp.ConsensusState, &consensusState)
	s.Require().NoError(err)

	wasmConsenusState, ok := consensusState.(*ibcwasmtypes.ConsensusState)
	s.Require().True(ok)

	var ethConsensusState ethereumligthclient.ConsensusState
	err = simd.Config().EncodingConfig.Codec.Unmarshal(wasmConsenusState.Data, &ethConsensusState)
	s.Require().NoError(err)

	return wasmConsenusState, ethConsensusState
}

func (s *FastSuite) LogVisualizerMessage(msg string) {
	if s.VisualizerClient != nil {
		fmt.Println("Visualizer message:", msg)
		s.VisualizerClient.SendMessage(msg, s.T().Name())
	}
}