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
)

const visualizerPort = 6969

// TestSuite is a suite of tests that require two chains and a relayer
type FastSuite struct {
	suite.Suite

	ChainA       ethereum.Ethereum
	ChainB       *cosmos.CosmosChain
	UserB        ibc.Wallet
	dockerClient *dockerclient.Client
	network      string
	logger       *zap.Logger
	ExecRep      *testreporter.RelayerExecReporter

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

	lastUnionUpdate uint64

	// The (hex encoded) checksum of the ethereum wasm client contract deployed on the Cosmos chain
	unionClientChecksum     string
	unionClientID           string
	tendermintClientID      string
	spec                    ethereum.Spec
	initialNextSyncComittee ethereum.SyncCommittee
}

// SetupSuite sets up the chains, relayer, user accounts, clients, and connections
func (s *FastSuite) SetupSuite(ctx context.Context) {
	t := s.T()

	chainSpecs := chainconfig.DefaultChainSpecs

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

		executionHeight, err := eth.BeaconAPIClient.GetExecutionHeight("finalized")
		s.Require().NoError(err)
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
		bootstrap, err := eth.BeaconAPIClient.GetBootstrap(header.Root)
		s.Require().NoError(err)

		//        assert!(bootstrap.header.beacon.slot == height.revision_height);
		if bootstrap.Data.Header.Beacon.Slot != executionHeight {
			s.Require().Fail(fmt.Sprintf("creating client: expected exec height %d, to equal boostrap slot %d", executionHeight, bootstrap.Data.Header.Beacon.Slot))
		}

		timestamp := bootstrap.Data.Header.Execution.Timestamp * 1_000_000_000
		stateRoot := ethereum.HexToBeBytes(bootstrap.Data.Header.Execution.StateRoot)

		currentPeriod := executionHeight / s.spec.Period()
		clientUpdates, err := eth.BeaconAPIClient.GetLightClientUpdates(currentPeriod, 1)
		s.Require().NoError(err)
		s.Require().NotEmpty(clientUpdates)
		update := clientUpdates[0]

		s.initialNextSyncComittee = update.Data.NextSyncCommittee
		s.lastUnionUpdate = bootstrap.Data.Header.Beacon.Slot
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

	s.Require().True(s.Run("Update client on Cosmos chain", func() {
		s.updateEthClient(ctx, sendBlockNumber, simdRelayerUser)
	}))

	var recvAck []byte
	var denomOnCosmos transfertypes.DenomTrace
	s.Require().True(s.Run("recvPacket on Cosmos chain", func() {
		proofHeight := clienttypes.Height{
			RevisionNumber: 0,
			RevisionHeight: s.lastUnionUpdate,
		}

		path := fmt.Sprintf("commitments/ports/%s/channels/%s/sequences/%d", sendPacket.SourcePort, sendPacket.SourceChannel, sendPacket.Sequence)
		storageKey := ethereum.GetStorageKey(path)
		storageKeys := []string{storageKey.Hex()}

		blockNumberHex := fmt.Sprintf("0x%x", s.lastUnionUpdate)
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
		storageProofBz := simd.Config().EncodingConfig.Codec.MustMarshal(&storageProof)

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

	var recvBlockNumber int64
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

		recvBlockNumber = receipt.BlockNumber.Int64()

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

	s.Require().True(s.Run("Update client on Cosmos chain", func() {
		s.updateEthClient(ctx, recvBlockNumber, simdRelayerUser)
	}))

	s.Require().True(s.Run("Acknowledge packet on Cosmos chain", func() {
		proofHeight := clienttypes.Height{
			RevisionNumber: 0,
			RevisionHeight: s.lastUnionUpdate,
		}

		path := fmt.Sprintf("acks/ports/%s/channels/%s/sequences/%d", returnPacket.DestinationPort, returnPacket.DestinationChannel, returnPacket.Sequence)
		storageKey := ethereum.GetStorageKey(path)
		storageKeys := []string{storageKey.Hex()}

		blockNumberHex := fmt.Sprintf("0x%x", s.lastUnionUpdate)
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
		storageProofBz := simd.Config().EncodingConfig.Codec.MustMarshal(&storageProof)

		txResp, err := s.BroadcastMessages(ctx, simd, cosmosUserWallet, 200_000, &channeltypes.MsgAcknowledgement{
			Packet:          returnPacket,
			Acknowledgement: returnWriteAckEvent.Acknowledgement,
			ProofAcked:      storageProofBz,
			ProofHeight:     proofHeight,
			Signer:          cosmosUserAddress,
		})
		s.Require().NoError(err)
		s.Require().Equal(uint32(0), txResp.Code)
	}))
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
