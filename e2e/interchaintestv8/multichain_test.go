package main

import (
	"context"
	"crypto/ecdsa"
	"encoding/hex"
	"fmt"
	"math/big"
	"os"
	"strconv"
	"strings"
	"testing"
	"time"

	"github.com/cosmos/gogoproto/proto"
	"github.com/stretchr/testify/suite"

	ethcommon "github.com/ethereum/go-ethereum/common"
	ethtypes "github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/crypto"

	sdkmath "cosmossdk.io/math"
	banktypes "cosmossdk.io/x/bank/types"

	codectypes "github.com/cosmos/cosmos-sdk/codec/types"
	sdk "github.com/cosmos/cosmos-sdk/types"

	transfertypes "github.com/cosmos/ibc-go/v9/modules/apps/transfer/types"
	clienttypes "github.com/cosmos/ibc-go/v9/modules/core/02-client/types"
	channeltypesv2 "github.com/cosmos/ibc-go/v9/modules/core/04-channel/v2/types"
	commitmenttypes "github.com/cosmos/ibc-go/v9/modules/core/23-commitment/types"
	commitmenttypesv2 "github.com/cosmos/ibc-go/v9/modules/core/23-commitment/types/v2"
	ibcexported "github.com/cosmos/ibc-go/v9/modules/core/exported"
	ibctm "github.com/cosmos/ibc-go/v9/modules/light-clients/07-tendermint"
	ibctesting "github.com/cosmos/ibc-go/v9/testing"

	"github.com/strangelove-ventures/interchaintest/v9/ibc"

	"github.com/cosmos/solidity-ibc-eureka/abigen/ibcerc20"
	"github.com/cosmos/solidity-ibc-eureka/abigen/ibcstore"
	"github.com/cosmos/solidity-ibc-eureka/abigen/ics02client"
	"github.com/cosmos/solidity-ibc-eureka/abigen/ics20lib"
	"github.com/cosmos/solidity-ibc-eureka/abigen/ics20transfer"
	"github.com/cosmos/solidity-ibc-eureka/abigen/ics26router"
	"github.com/cosmos/solidity-ibc-eureka/abigen/sp1ics07tendermint"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/chainconfig"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/e2esuite"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/ethereum"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/operator"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/relayer"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/erc20"
	relayertypes "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/relayer"
)

type MultichainTestSuite struct {
	e2esuite.TestSuite

	// The private key of a test account
	key *ecdsa.PrivateKey
	// The private key of the faucet account of interchaintest
	deployer *ecdsa.PrivateKey

	contractAddresses     ethereum.DeployedContracts
	chainBSP1Ics07Address string

	chainASP1Ics07Contract *sp1ics07tendermint.Contract
	chainBSP1Ics07Contract *sp1ics07tendermint.Contract
	ics02Contract          *ics02client.Contract
	ics26Contract          *ics26router.Contract
	ics20Contract          *ics20transfer.Contract
	erc20Contract          *erc20.Contract
	ibcStoreContract       *ibcstore.Contract
	escrowContractAddr     ethcommon.Address

	EthToChainARelayerClient    relayertypes.RelayerServiceClient
	ChainAToEthRelayerClient    relayertypes.RelayerServiceClient
	EthToChainBRelayerClient    relayertypes.RelayerServiceClient
	ChainBToEthRelayerClient    relayertypes.RelayerServiceClient
	ChainAToChainBRelayerClient relayertypes.RelayerServiceClient
	ChainBToChainARelayerClient relayertypes.RelayerServiceClient

	SimdARelayerSubmitter ibc.Wallet
	SimdBRelayerSubmitter ibc.Wallet
	EthRelayerSubmitter   *ecdsa.PrivateKey
}

// TestWithMultichainTestSuite is the boilerplate code that allows the test suite to be run
func TestWithMultichainTestSuite(t *testing.T) {
	suite.Run(t, new(MultichainTestSuite))
}

func (s *MultichainTestSuite) SetupSuite(ctx context.Context, proofType operator.SupportedProofType) {
	chainconfig.DefaultChainSpecs = append(chainconfig.DefaultChainSpecs, chainconfig.IbcGoChainSpec("ibc-go-simd-2", "simd-2"))

	s.TestSuite.SetupSuite(ctx)

	eth, simdA, simdB := s.EthChain, s.CosmosChains[0], s.CosmosChains[1]

	var prover string
	s.Require().True(s.Run("Set up environment", func() {
		err := os.Chdir("../..")
		s.Require().NoError(err)

		s.key, err = eth.CreateAndFundUser()
		s.Require().NoError(err)

		s.EthRelayerSubmitter, err = eth.CreateAndFundUser()
		s.Require().NoError(err)

		operatorKey, err := eth.CreateAndFundUser()
		s.Require().NoError(err)

		s.deployer, err = eth.CreateAndFundUser()
		s.Require().NoError(err)

		s.SimdARelayerSubmitter = s.CreateAndFundCosmosUser(ctx, simdA)
		s.SimdBRelayerSubmitter = s.CreateAndFundCosmosUser(ctx, simdB)

		prover = os.Getenv(testvalues.EnvKeySp1Prover)
		switch prover {
		case "", testvalues.EnvValueSp1Prover_Mock:
			s.T().Logf("Using mock prover")
			prover = testvalues.EnvValueSp1Prover_Mock
			os.Setenv(testvalues.EnvKeySp1Prover, testvalues.EnvValueSp1Prover_Mock)
			os.Setenv(testvalues.EnvKeyVerifier, testvalues.EnvValueVerifier_Mock)

			s.Require().Empty(
				os.Getenv(testvalues.EnvKeyGenerateSolidityFixtures),
				"Fixtures are not supported for mock prover",
			)
		case testvalues.EnvValueSp1Prover_Network:
			s.Require().Empty(
				os.Getenv(testvalues.EnvKeyVerifier),
				fmt.Sprintf("%s should not be set when using the network prover in e2e tests.", testvalues.EnvKeyVerifier),
			)
		default:
			s.Require().Fail("invalid prover type: %s", prover)
		}

		os.Setenv(testvalues.EnvKeyRustLog, testvalues.EnvValueRustLog_Info)
		os.Setenv(testvalues.EnvKeyEthRPC, eth.RPC)
		os.Setenv(testvalues.EnvKeySp1Prover, prover)
		os.Setenv(testvalues.EnvKeyOperatorPrivateKey, hex.EncodeToString(crypto.FromECDSA(operatorKey)))
	}))

	s.Require().True(s.Run("Deploy ethereum contracts with SimdA client", func() {
		os.Setenv(testvalues.EnvKeyTendermintRPC, simdA.GetHostRPCAddress())

		args := append([]string{
			"--trust-level", testvalues.DefaultTrustLevel.String(),
			"--trusting-period", strconv.Itoa(testvalues.DefaultTrustPeriod),
			"-o", testvalues.Sp1GenesisFilePath,
		}, proofType.ToOperatorArgs()...)
		s.Require().NoError(operator.RunGenesis(args...))

		var (
			stdout []byte
			err    error
		)
		switch prover {
		case testvalues.EnvValueSp1Prover_Mock:
			stdout, err = eth.ForgeScript(s.deployer, testvalues.E2EDeployScriptPath)
			s.Require().NoError(err)
		case testvalues.EnvValueSp1Prover_Network:
			// make sure that the NETWORK_PRIVATE_KEY is set.
			s.Require().NotEmpty(os.Getenv(testvalues.EnvKeyNetworkPrivateKey))

			stdout, err = eth.ForgeScript(s.deployer, testvalues.E2EDeployScriptPath)
			s.Require().NoError(err)
		default:
			s.Require().Fail("invalid prover type: %s", prover)
		}

		s.contractAddresses, err = ethereum.GetEthContractsFromDeployOutput(string(stdout))
		s.Require().NoError(err)
		s.chainASP1Ics07Contract, err = sp1ics07tendermint.NewContract(ethcommon.HexToAddress(s.contractAddresses.Ics07Tendermint), eth.RPCClient)
		s.Require().NoError(err)
		s.ics02Contract, err = ics02client.NewContract(ethcommon.HexToAddress(s.contractAddresses.Ics02Client), eth.RPCClient)
		s.Require().NoError(err)
		s.ics26Contract, err = ics26router.NewContract(ethcommon.HexToAddress(s.contractAddresses.Ics26Router), eth.RPCClient)
		s.Require().NoError(err)
		s.ics20Contract, err = ics20transfer.NewContract(ethcommon.HexToAddress(s.contractAddresses.Ics20Transfer), eth.RPCClient)
		s.Require().NoError(err)
		s.erc20Contract, err = erc20.NewContract(ethcommon.HexToAddress(s.contractAddresses.Erc20), eth.RPCClient)
		s.Require().NoError(err)
		s.escrowContractAddr = ethcommon.HexToAddress(s.contractAddresses.Escrow)
		s.ibcStoreContract, err = ibcstore.NewContract(ethcommon.HexToAddress(s.contractAddresses.IbcStore), eth.RPCClient)
		s.Require().NoError(err)
	}))

	s.Require().True(s.Run("Deploy SimdB light client on ethereum", func() {
		os.Setenv(testvalues.EnvKeyTendermintRPC, simdB.GetHostRPCAddress())

		args := append([]string{
			"--trust-level", testvalues.DefaultTrustLevel.String(),
			"--trusting-period", strconv.Itoa(testvalues.DefaultTrustPeriod),
			"-o", testvalues.Sp1GenesisFilePath,
		}, proofType.ToOperatorArgs()...)
		s.Require().NoError(operator.RunGenesis(args...))

		s.T().Cleanup(func() {
			_ = os.Remove(testvalues.Sp1GenesisFilePath)
		})

		var (
			stdout []byte
			err    error
		)
		switch prover {
		case testvalues.EnvValueSp1Prover_Mock:
			stdout, err = eth.ForgeScript(s.deployer, testvalues.SP1ICS07DeployScriptPath, "--json")
			s.Require().NoError(err)
		case testvalues.EnvValueSp1Prover_Network:
			// make sure that the NETWORK_PRIVATE_KEY is set.
			s.Require().NotEmpty(os.Getenv(testvalues.EnvKeyNetworkPrivateKey))

			stdout, err = eth.ForgeScript(s.deployer, testvalues.SP1ICS07DeployScriptPath, "--json")
			s.Require().NoError(err)
		default:
			s.Require().Fail("invalid prover type: %s", prover)
		}

		s.chainBSP1Ics07Address, err = ethereum.GetOnlySp1Ics07AddressFromStdout(string(stdout))
		s.Require().NoError(err)
		s.Require().NotEmpty(s.chainBSP1Ics07Address)
		s.Require().True(ethcommon.IsHexAddress(s.chainBSP1Ics07Address))

		s.chainBSP1Ics07Contract, err = sp1ics07tendermint.NewContract(ethcommon.HexToAddress(s.chainBSP1Ics07Address), eth.RPCClient)
		s.Require().NoError(err)
	}))

	s.Require().True(s.Run("Fund address with ERC20", func() {
		tx, err := s.erc20Contract.Transfer(s.GetTransactOpts(eth.Faucet, eth), crypto.PubkeyToAddress(s.key.PublicKey), testvalues.StartingERC20Balance)
		s.Require().NoError(err)

		_, err = eth.GetTxReciept(ctx, tx.Hash()) // wait for the tx to be mined
		s.Require().NoError(err)
	}))

	s.Require().True(s.Run("Add ethereum light client on SimdA", func() {
		s.CreateEthereumLightClient(ctx, simdA, s.SimdARelayerSubmitter, s.contractAddresses.IbcStore)
	}))

	s.Require().True(s.Run("Add simdA client and counterparty on EVM", func() {
		counterpartyInfo := ics02client.IICS02ClientMsgsCounterpartyInfo{
			ClientId:     ibctesting.FirstChannelID,
			MerklePrefix: [][]byte{[]byte(ibcexported.StoreKey), []byte("")},
		}
		lightClientAddress := ethcommon.HexToAddress(s.contractAddresses.Ics07Tendermint)
		tx, err := s.ics02Contract.AddClient(s.GetTransactOpts(s.deployer, eth), ibcexported.Tendermint, counterpartyInfo, lightClientAddress)
		s.Require().NoError(err)

		receipt, err := eth.GetTxReciept(ctx, tx.Hash())
		s.Require().NoError(err)

		event, err := e2esuite.GetEvmEvent(receipt, s.ics02Contract.ParseICS02ClientAdded)
		s.Require().NoError(err)
		s.Require().Equal(ibctesting.FirstClientID, event.ClientId)
		s.Require().Equal(ibctesting.FirstChannelID, event.CounterpartyInfo.ClientId)
	}))

	s.Require().True(s.Run("Add ethereum light client on SimdB", func() {
		s.CreateEthereumLightClient(ctx, simdB, s.SimdBRelayerSubmitter, s.contractAddresses.IbcStore)
	}))

	s.Require().True(s.Run("Add simdB client and counterparty on EVM", func() {
		counterpartyInfo := ics02client.IICS02ClientMsgsCounterpartyInfo{
			ClientId:     ibctesting.FirstChannelID,
			MerklePrefix: [][]byte{[]byte(ibcexported.StoreKey), []byte("")},
		}
		lightClientAddress := ethcommon.HexToAddress(s.chainBSP1Ics07Address)
		tx, err := s.ics02Contract.AddClient(s.GetTransactOpts(s.deployer, eth), ibcexported.Tendermint, counterpartyInfo, lightClientAddress)
		s.Require().NoError(err)

		receipt, err := eth.GetTxReciept(ctx, tx.Hash())
		s.Require().NoError(err)

		event, err := e2esuite.GetEvmEvent(receipt, s.ics02Contract.ParseICS02ClientAdded)
		s.Require().NoError(err)
		s.Require().Equal(ibctesting.SecondClientID, event.ClientId)
		s.Require().Equal(ibctesting.FirstChannelID, event.CounterpartyInfo.ClientId)
	}))

	s.Require().True(s.Run("Create channel and register counterparty on SimdA", func() {
		merklePathPrefix := commitmenttypesv2.NewMerklePath([]byte(""))

		_, err := s.BroadcastMessages(ctx, simdA, s.SimdARelayerSubmitter, 200_000, &channeltypesv2.MsgCreateChannel{
			ClientId:         s.EthereumLightClientID,
			MerklePathPrefix: merklePathPrefix,
			Signer:           s.SimdARelayerSubmitter.FormattedAddress(),
		}, &channeltypesv2.MsgRegisterCounterparty{
			ChannelId:             ibctesting.FirstChannelID,
			CounterpartyChannelId: ibctesting.FirstClientID,
			Signer:                s.SimdARelayerSubmitter.FormattedAddress(),
		})
		s.Require().NoError(err)
	}))

	s.Require().True(s.Run("Create channel and register counterparty on SimdB", func() {
		merklePathPrefix := commitmenttypesv2.NewMerklePath([]byte(""))

		_, err := s.BroadcastMessages(ctx, simdB, s.SimdBRelayerSubmitter, 200_000, &channeltypesv2.MsgCreateChannel{
			ClientId:         s.EthereumLightClientID,
			MerklePathPrefix: merklePathPrefix,
			Signer:           s.SimdBRelayerSubmitter.FormattedAddress(),
		}, &channeltypesv2.MsgRegisterCounterparty{
			ChannelId:             ibctesting.FirstChannelID,
			CounterpartyChannelId: ibctesting.SecondClientID,
			Signer:                s.SimdBRelayerSubmitter.FormattedAddress(),
		})
		s.Require().NoError(err)
	}))

	s.Require().True(s.Run("Create Light Client of Chain A on Chain B", func() {
		simdAHeader, err := s.FetchCosmosHeader(ctx, simdA)
		s.Require().NoError(err)

		var (
			clientStateAny    *codectypes.Any
			consensusStateAny *codectypes.Any
		)
		s.Require().True(s.Run("Construct the client and consensus state", func() {
			tmConfig := ibctesting.NewTendermintConfig()
			revision := clienttypes.ParseChainID(simdAHeader.ChainID)
			height := clienttypes.NewHeight(revision, uint64(simdAHeader.Height))

			clientState := ibctm.NewClientState(
				simdAHeader.ChainID,
				tmConfig.TrustLevel, tmConfig.TrustingPeriod, tmConfig.UnbondingPeriod, tmConfig.MaxClockDrift,
				height, commitmenttypes.GetSDKSpecs(), ibctesting.UpgradePath,
			)
			clientStateAny, err = codectypes.NewAnyWithValue(clientState)
			s.Require().NoError(err)

			consensusState := ibctm.NewConsensusState(simdAHeader.Time, commitmenttypes.NewMerkleRoot([]byte(ibctm.SentinelRoot)), simdAHeader.ValidatorsHash)
			consensusStateAny, err = codectypes.NewAnyWithValue(consensusState)
			s.Require().NoError(err)
		}))

		_, err = s.BroadcastMessages(ctx, simdB, s.SimdBRelayerSubmitter, 2_000_000, &clienttypes.MsgCreateClient{
			ClientState:    clientStateAny,
			ConsensusState: consensusStateAny,
			Signer:         s.SimdBRelayerSubmitter.FormattedAddress(),
		})
		s.Require().NoError(err)
	}))

	s.Require().True(s.Run("Create Light Client of Chain B on Chain A", func() {
		simdBHeader, err := s.FetchCosmosHeader(ctx, simdB)
		s.Require().NoError(err)

		var (
			clientStateAny    *codectypes.Any
			consensusStateAny *codectypes.Any
		)
		s.Require().True(s.Run("Construct the client and consensus state", func() {
			tmConfig := ibctesting.NewTendermintConfig()
			revision := clienttypes.ParseChainID(simdBHeader.ChainID)
			height := clienttypes.NewHeight(revision, uint64(simdBHeader.Height))

			clientState := ibctm.NewClientState(
				simdBHeader.ChainID,
				tmConfig.TrustLevel, tmConfig.TrustingPeriod, tmConfig.UnbondingPeriod, tmConfig.MaxClockDrift,
				height, commitmenttypes.GetSDKSpecs(), ibctesting.UpgradePath,
			)
			clientStateAny, err = codectypes.NewAnyWithValue(clientState)
			s.Require().NoError(err)

			consensusState := ibctm.NewConsensusState(simdBHeader.Time, commitmenttypes.NewMerkleRoot([]byte(ibctm.SentinelRoot)), simdBHeader.ValidatorsHash)
			consensusStateAny, err = codectypes.NewAnyWithValue(consensusState)
			s.Require().NoError(err)
		}))

		_, err = s.BroadcastMessages(ctx, simdA, s.SimdARelayerSubmitter, 2_000_000, &clienttypes.MsgCreateClient{
			ClientState:    clientStateAny,
			ConsensusState: consensusStateAny,
			Signer:         s.SimdARelayerSubmitter.FormattedAddress(),
		})
		s.Require().NoError(err)
	}))

	s.Require().True(s.Run("Create Channel and register counterparty on Chain A", func() {
		merklePathPrefix := commitmenttypesv2.NewMerklePath([]byte(ibcexported.StoreKey), []byte(""))

		// We can do this because we know what the counterparty channel ID will be
		_, err := s.BroadcastMessages(ctx, simdA, s.SimdARelayerSubmitter, 200_000, &channeltypesv2.MsgCreateChannel{
			ClientId:         ibctesting.SecondClientID,
			MerklePathPrefix: merklePathPrefix,
			Signer:           s.SimdARelayerSubmitter.FormattedAddress(),
		}, &channeltypesv2.MsgRegisterCounterparty{
			ChannelId:             ibctesting.SecondChannelID,
			CounterpartyChannelId: ibctesting.SecondChannelID,
			Signer:                s.SimdARelayerSubmitter.FormattedAddress(),
		})
		s.Require().NoError(err)
	}))

	s.Require().True(s.Run("Create Channel and register counterparty on Chain B", func() {
		merklePathPrefix := commitmenttypesv2.NewMerklePath([]byte(ibcexported.StoreKey), []byte(""))

		_, err := s.BroadcastMessages(ctx, simdB, s.SimdBRelayerSubmitter, 200_000, &channeltypesv2.MsgCreateChannel{
			ClientId:         ibctesting.SecondClientID,
			MerklePathPrefix: merklePathPrefix,
			Signer:           s.SimdBRelayerSubmitter.FormattedAddress(),
		}, &channeltypesv2.MsgRegisterCounterparty{
			ChannelId:             ibctesting.SecondChannelID,
			CounterpartyChannelId: ibctesting.SecondChannelID,
			Signer:                s.SimdBRelayerSubmitter.FormattedAddress(),
		})
		s.Require().NoError(err)
	}))

	var relayerProcess *os.Process
	var configInfo relayer.MultichainConfigInfo
	s.Require().True(s.Run("Start Relayer", func() {
		beaconAPI := ""
		// The BeaconAPIClient is nil when the testnet is `pow`
		if eth.BeaconAPIClient != nil {
			beaconAPI = eth.BeaconAPIClient.GetBeaconAPIURL()
		}

		configInfo = relayer.MultichainConfigInfo{
			EthToChainAPort:     3000,
			ChainAToEthPort:     3001,
			EthToChainBPort:     3002,
			ChainBToEthPort:     3003,
			ChainAToChainBPort:  3004,
			ChainBToChainAPort:  3005,
			ChainATmRPC:         simdA.GetHostRPCAddress(),
			ChainASignerAddress: s.SimdARelayerSubmitter.FormattedAddress(),
			ChainBTmRPC:         simdB.GetHostRPCAddress(),
			ChainBSignerAddress: s.SimdBRelayerSubmitter.FormattedAddress(),
			ICS26Address:        s.contractAddresses.Ics26Router,
			EthRPC:              eth.RPC,
			BeaconAPI:           beaconAPI,
			SP1PrivateKey:       os.Getenv(testvalues.EnvKeyNetworkPrivateKey),
			MockWasmClient:      os.Getenv(testvalues.EnvKeyEthTestnetType) == testvalues.EthTestnetTypePoW,
			MockSP1Client:       prover == testvalues.EnvValueSp1Prover_Mock,
		}

		err := configInfo.GenerateMultichainConfigFile(testvalues.RelayerConfigFilePath)
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

	s.Require().True(s.Run("Create Relayer Clients", func() {
		var err error
		s.EthToChainARelayerClient, err = relayer.GetGRPCClient(configInfo.EthToChainAGRPCAddress())
		s.Require().NoError(err)

		s.ChainAToEthRelayerClient, err = relayer.GetGRPCClient(configInfo.ChainAToEthGRPCAddress())
		s.Require().NoError(err)

		s.EthToChainBRelayerClient, err = relayer.GetGRPCClient(configInfo.EthToChainBGRPCAddress())
		s.Require().NoError(err)

		s.ChainBToEthRelayerClient, err = relayer.GetGRPCClient(configInfo.ChainBToEthGRPCAddress())
		s.Require().NoError(err)

		s.ChainAToChainBRelayerClient, err = relayer.GetGRPCClient(configInfo.ChainAToChainBGRPCAddress())
		s.Require().NoError(err)

		s.ChainBToChainARelayerClient, err = relayer.GetGRPCClient(configInfo.ChainBToChainAGRPCAddress())
		s.Require().NoError(err)
	}))
}

func (s *MultichainTestSuite) TestDeploy_Groth16() {
	ctx := context.Background()
	proofType := operator.ProofTypeGroth16

	s.SetupSuite(ctx, proofType)

	eth, simdA, simdB := s.EthChain, s.CosmosChains[0], s.CosmosChains[1]

	s.Require().True(s.Run("Verify SimdA SP1 Client", func() {
		clientState, err := s.chainASP1Ics07Contract.ClientState(nil)
		s.Require().NoError(err)

		stakingParams, err := simdA.StakingQueryParams(ctx)
		s.Require().NoError(err)

		s.Require().Equal(simdA.Config().ChainID, clientState.ChainId)
		s.Require().Equal(uint8(testvalues.DefaultTrustLevel.Numerator), clientState.TrustLevel.Numerator)
		s.Require().Equal(uint8(testvalues.DefaultTrustLevel.Denominator), clientState.TrustLevel.Denominator)
		s.Require().Equal(uint32(testvalues.DefaultTrustPeriod), clientState.TrustingPeriod)
		s.Require().Equal(uint32(stakingParams.UnbondingTime.Seconds()), clientState.UnbondingPeriod)
		s.Require().False(clientState.IsFrozen)
		s.Require().Equal(uint32(1), clientState.LatestHeight.RevisionNumber)
		s.Require().Greater(clientState.LatestHeight.RevisionHeight, uint32(0))
	}))

	s.Require().True(s.Run("Verify SimdB SP1 Client", func() {
		clientState, err := s.chainBSP1Ics07Contract.ClientState(nil)
		s.Require().NoError(err)

		stakingParams, err := simdB.StakingQueryParams(ctx)
		s.Require().NoError(err)

		s.Require().Equal(simdB.Config().ChainID, clientState.ChainId)
		s.Require().Equal(uint8(testvalues.DefaultTrustLevel.Numerator), clientState.TrustLevel.Numerator)
		s.Require().Equal(uint8(testvalues.DefaultTrustLevel.Denominator), clientState.TrustLevel.Denominator)
		s.Require().Equal(uint32(testvalues.DefaultTrustPeriod), clientState.TrustingPeriod)
		s.Require().Equal(uint32(stakingParams.UnbondingTime.Seconds()), clientState.UnbondingPeriod)
		s.Require().False(clientState.IsFrozen)
		s.Require().Equal(uint32(2), clientState.LatestHeight.RevisionNumber)
		s.Require().Greater(clientState.LatestHeight.RevisionHeight, uint32(0))
	}))

	s.Require().True(s.Run("Verify ICS02 Client", func() {
		isAdmin, err := s.ics02Contract.HasRole(nil, testvalues.DefaultAdminRole, crypto.PubkeyToAddress(s.deployer.PublicKey))
		s.Require().NoError(err)
		s.Require().True(isAdmin)

		clientAddress, err := s.ics02Contract.GetClient(nil, ibctesting.FirstClientID)
		s.Require().NoError(err)
		s.Require().Equal(s.contractAddresses.Ics07Tendermint, strings.ToLower(clientAddress.Hex()))

		counterpartyInfo, err := s.ics02Contract.GetCounterparty(nil, ibctesting.FirstClientID)
		s.Require().NoError(err)
		s.Require().Equal(ibctesting.FirstChannelID, counterpartyInfo.ClientId)

		clientAddress, err = s.ics02Contract.GetClient(nil, ibctesting.SecondClientID)
		s.Require().NoError(err)
		s.Require().Equal(s.chainBSP1Ics07Address, strings.ToLower(clientAddress.Hex()))

		counterpartyInfo, err = s.ics02Contract.GetCounterparty(nil, ibctesting.SecondClientID)
		s.Require().NoError(err)
		s.Require().Equal(ibctesting.FirstChannelID, counterpartyInfo.ClientId)
	}))

	s.Require().True(s.Run("Verify ICS26 Router", func() {
		var portCustomizerRole [32]byte
		copy(portCustomizerRole[:], crypto.Keccak256([]byte("PORT_CUSTOMIZER_ROLE")))

		hasRole, err := s.ics26Contract.HasRole(nil, portCustomizerRole, crypto.PubkeyToAddress(s.deployer.PublicKey))
		s.Require().NoError(err)
		s.Require().True(hasRole)

		transferAddress, err := s.ics26Contract.GetIBCApp(nil, transfertypes.PortID)
		s.Require().NoError(err)
		s.Require().Equal(s.contractAddresses.Ics20Transfer, strings.ToLower(transferAddress.Hex()))
	}))

	s.Require().True(s.Run("Verify ERC20 Genesis", func() {
		userBalance, err := s.erc20Contract.BalanceOf(nil, crypto.PubkeyToAddress(s.key.PublicKey))
		s.Require().NoError(err)
		s.Require().Equal(testvalues.StartingERC20Balance, userBalance)
	}))

	s.Require().True(s.Run("Verify ethereum light client for SimdA", func() {
		_, err := e2esuite.GRPCQuery[clienttypes.QueryClientStateResponse](ctx, simdA, &clienttypes.QueryClientStateRequest{
			ClientId: s.EthereumLightClientID,
		})
		s.Require().NoError(err)

		channelResp, err := e2esuite.GRPCQuery[channeltypesv2.QueryChannelResponse](ctx, simdA, &channeltypesv2.QueryChannelRequest{
			ChannelId: ibctesting.FirstChannelID,
		})
		s.Require().NoError(err)
		s.Require().Equal(s.EthereumLightClientID, channelResp.Channel.ClientId)
		s.Require().Equal(ibctesting.FirstClientID, channelResp.Channel.CounterpartyChannelId)
	}))

	s.Require().True(s.Run("Verify ethereum light client for SimdB", func() {
		_, err := e2esuite.GRPCQuery[clienttypes.QueryClientStateResponse](ctx, simdB, &clienttypes.QueryClientStateRequest{
			ClientId: s.EthereumLightClientID,
		})
		s.Require().NoError(err)

		channelResp, err := e2esuite.GRPCQuery[channeltypesv2.QueryChannelResponse](ctx, simdB, &channeltypesv2.QueryChannelRequest{
			ChannelId: ibctesting.FirstChannelID,
		})
		s.Require().NoError(err)
		s.Require().Equal(s.EthereumLightClientID, channelResp.Channel.ClientId)
		s.Require().Equal(ibctesting.SecondClientID, channelResp.Channel.CounterpartyChannelId)
	}))

	s.Require().True(s.Run("Verify Light Client of Chain A on Chain B", func() {
		clientStateResp, err := e2esuite.GRPCQuery[clienttypes.QueryClientStateResponse](ctx, simdB, &clienttypes.QueryClientStateRequest{
			ClientId: ibctesting.SecondClientID,
		})
		s.Require().NoError(err)
		s.Require().NotZero(clientStateResp.ClientState.Value)

		var clientState ibctm.ClientState
		err = proto.Unmarshal(clientStateResp.ClientState.Value, &clientState)
		s.Require().NoError(err)
		s.Require().Equal(simdA.Config().ChainID, clientState.ChainId)
	}))

	s.Require().True(s.Run("Verify Light Client of Chain B on Chain A", func() {
		clientStateResp, err := e2esuite.GRPCQuery[clienttypes.QueryClientStateResponse](ctx, simdA, &clienttypes.QueryClientStateRequest{
			ClientId: ibctesting.SecondClientID,
		})
		s.Require().NoError(err)
		s.Require().NotZero(clientStateResp.ClientState.Value)

		var clientState ibctm.ClientState
		err = proto.Unmarshal(clientStateResp.ClientState.Value, &clientState)
		s.Require().NoError(err)
		s.Require().Equal(simdB.Config().ChainID, clientState.ChainId)
	}))

	s.Require().True(s.Run("Verify SimdA to Eth Relayer Info", func() {
		info, err := s.ChainAToEthRelayerClient.Info(context.Background(), &relayertypes.InfoRequest{})
		s.Require().NoError(err)
		s.Require().NotNil(info)
		s.Require().Equal(simdA.Config().ChainID, info.SourceChain.ChainId)
		s.Require().Equal(eth.ChainID.String(), info.TargetChain.ChainId)
	}))

	s.Require().True(s.Run("Verify Eth to SimdA Relayer Info", func() {
		info, err := s.EthToChainARelayerClient.Info(context.Background(), &relayertypes.InfoRequest{})
		s.Require().NoError(err)
		s.Require().NotNil(info)
		s.Require().Equal(eth.ChainID.String(), info.SourceChain.ChainId)
		s.Require().Equal(simdA.Config().ChainID, info.TargetChain.ChainId)
	}))

	s.Require().True(s.Run("Verify SimdB to Eth Relayer Info", func() {
		info, err := s.ChainBToEthRelayerClient.Info(context.Background(), &relayertypes.InfoRequest{})
		s.Require().NoError(err)
		s.Require().NotNil(info)
		s.Require().Equal(simdB.Config().ChainID, info.SourceChain.ChainId)
		s.Require().Equal(eth.ChainID.String(), info.TargetChain.ChainId)
	}))

	s.Require().True(s.Run("Verify Eth to SimdB Relayer Info", func() {
		info, err := s.EthToChainBRelayerClient.Info(context.Background(), &relayertypes.InfoRequest{})
		s.Require().NoError(err)
		s.Require().NotNil(info)
		s.Require().Equal(eth.ChainID.String(), info.SourceChain.ChainId)
		s.Require().Equal(simdB.Config().ChainID, info.TargetChain.ChainId)
	}))

	s.Require().True(s.Run("Verify Chain A to Chain B Relayer Info", func() {
		info, err := s.ChainAToChainBRelayerClient.Info(context.Background(), &relayertypes.InfoRequest{})
		s.Require().NoError(err)
		s.Require().NotNil(info)
		s.Require().Equal(simdA.Config().ChainID, info.SourceChain.ChainId)
		s.Require().Equal(simdB.Config().ChainID, info.TargetChain.ChainId)
	}))

	s.Require().True(s.Run("Verify Chain B to Chain A Relayer Info", func() {
		info, err := s.ChainBToChainARelayerClient.Info(context.Background(), &relayertypes.InfoRequest{})
		s.Require().NoError(err)
		s.Require().NotNil(info)
		s.Require().Equal(simdB.Config().ChainID, info.SourceChain.ChainId)
		s.Require().Equal(simdA.Config().ChainID, info.TargetChain.ChainId)
	}))
}

func (s *MultichainTestSuite) TestTransferCosmosToEthToCosmos_Groth16() {
	ctx := context.Background()
	proofType := operator.ProofTypeGroth16

	s.SetupSuite(ctx, proofType)

	eth, simdA, simdB := s.EthChain, s.CosmosChains[0], s.CosmosChains[1]

	ics26Address := ethcommon.HexToAddress(s.contractAddresses.Ics26Router)
	ics20Address := ethcommon.HexToAddress(s.contractAddresses.Ics20Transfer)
	transferAmount := big.NewInt(testvalues.TransferAmount)
	ethereumUserAddress := crypto.PubkeyToAddress(s.key.PublicKey)
	simdAUser, simdBUser := s.CosmosUsers[0], s.CosmosUsers[1]

	var simdASendTxHash []byte
	s.Require().True(s.Run("Send transfer on SimdA chain", func() {
		timeout := uint64(time.Now().Add(30 * time.Minute).Unix())
		transferCoin := sdk.NewCoin(simdA.Config().Denom, sdkmath.NewIntFromBigInt(transferAmount))

		transferPayload := ics20lib.ICS20LibFungibleTokenPacketData{
			Denom:    transferCoin.Denom,
			Amount:   transferCoin.Amount.BigInt(),
			Sender:   simdAUser.FormattedAddress(),
			Receiver: strings.ToLower(ethereumUserAddress.Hex()),
			Memo:     "",
		}
		transferBz, err := ics20lib.EncodeFungibleTokenPacketData(transferPayload)
		s.Require().NoError(err)

		payload := channeltypesv2.Payload{
			SourcePort:      transfertypes.PortID,
			DestinationPort: transfertypes.PortID,
			Version:         transfertypes.V1,
			Encoding:        transfertypes.EncodingABI,
			Value:           transferBz,
		}
		msgSendPacket := channeltypesv2.MsgSendPacket{
			SourceChannel:    ibctesting.FirstChannelID,
			TimeoutTimestamp: timeout,
			Payloads: []channeltypesv2.Payload{
				payload,
			},
			Signer: simdAUser.FormattedAddress(),
		}

		resp, err := s.BroadcastMessages(ctx, simdA, simdAUser, 200_000, &msgSendPacket)
		s.Require().NoError(err)
		s.Require().NotEmpty(resp.TxHash)

		simdASendTxHash, err = hex.DecodeString(resp.TxHash)
		s.Require().NoError(err)

		s.Require().True(s.Run("Verify balances on Cosmos chain", func() {
			// Check the balance of UserB
			resp, err := e2esuite.GRPCQuery[banktypes.QueryBalanceResponse](ctx, simdA, &banktypes.QueryBalanceRequest{
				Address: simdAUser.FormattedAddress(),
				Denom:   transferCoin.Denom,
			})
			s.Require().NoError(err)
			s.Require().NotNil(resp.Balance)
			s.Require().Equal(testvalues.InitialBalance-testvalues.TransferAmount, resp.Balance.Amount.Int64())
		}))
	}))

	var (
		ibcERC20        *ibcerc20.Contract
		ibcERC20Address string
	)
	s.Require().True(s.Run("Receive packet on Ethereum", func() {
		var recvRelayTx []byte
		s.Require().True(s.Run("Retrieve relay tx", func() {
			resp, err := s.ChainAToEthRelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
				SourceTxIds:     [][]byte{simdASendTxHash},
				TargetChannelId: ibctesting.FirstClientID,
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Tx)
			s.Require().Equal(resp.Address, ics26Address.String())

			recvRelayTx = resp.Tx
		}))

		var packet ics26router.IICS26RouterMsgsPacket
		s.Require().True(s.Run("Submit relay tx", func() {
			receipt, err := eth.BroadcastTx(ctx, s.EthRelayerSubmitter, 5_000_000, ics26Address, recvRelayTx)
			s.Require().NoError(err)
			s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status, fmt.Sprintf("Tx failed: %+v", receipt))

			ethReceiveAckEvent, err := e2esuite.GetEvmEvent(receipt, s.ics26Contract.ParseWriteAcknowledgement)
			s.Require().NoError(err)

			packet = ethReceiveAckEvent.Packet
			// ackTxHash = receipt.TxHash.Bytes()
			// NOTE: ackTxHash is not used in the test since acking the packet is not necessary
		}))

		// Recreate the full denom path
		transferCoin := sdk.NewCoin(simdA.Config().Denom, sdkmath.NewIntFromBigInt(transferAmount))
		denomOnEthereum := transfertypes.NewDenom(transferCoin.Denom, transfertypes.NewHop(packet.Payloads[0].DestPort, packet.DestChannel))

		ibcERC20EthAddress, err := s.ics20Contract.IbcERC20Contract(nil, denomOnEthereum.IBCDenom())
		s.Require().NoError(err)

		ibcERC20Address = ibcERC20EthAddress.Hex()
		s.Require().NotEmpty(ibcERC20Address)

		ibcERC20, err = ibcerc20.NewContract(ethcommon.HexToAddress(ibcERC20Address), eth.RPCClient)
		s.Require().NoError(err)

		actualFullDenom, err := ibcERC20.FullDenomPath(nil)
		s.Require().NoError(err)
		s.Require().Equal(denomOnEthereum.Path(), actualFullDenom)

		s.True(s.Run("Verify balances on Ethereum", func() {
			// User balance on Ethereum
			userBalance, err := ibcERC20.BalanceOf(nil, ethereumUserAddress)
			s.Require().NoError(err)
			s.Require().Equal(transferAmount, userBalance)

			// ICS20 contract balance on Ethereum
			ics20TransferBalance, err := ibcERC20.BalanceOf(nil, ics20Address)
			s.Require().NoError(err)
			s.Require().Zero(ics20TransferBalance.Int64())
		}))
	}))

	s.Require().True(s.Run("Approve the ICS20Transfer.sol contract to spend the erc20 tokens", func() {
		tx, err := ibcERC20.Approve(s.GetTransactOpts(s.key, eth), ics20Address, transferAmount)
		s.Require().NoError(err)

		receipt, err := eth.GetTxReciept(ctx, tx.Hash())
		s.Require().NoError(err)
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)

		allowance, err := ibcERC20.Allowance(nil, ethereumUserAddress, ics20Address)
		s.Require().NoError(err)
		s.Require().Equal(transferAmount, allowance)
	}))

	var ethSendTxHash []byte
	s.Require().True(s.Run("Transfer tokens from Ethereum to SimdB", func() {
		timeout := uint64(time.Now().Add(30 * time.Minute).Unix())
		msgSendPacket := s.createICS20MsgSendPacket(
			ethereumUserAddress,
			ibcERC20Address,
			transferAmount,
			simdBUser.FormattedAddress(),
			ibctesting.SecondClientID,
			timeout,
			"",
		)

		tx, err := s.ics26Contract.SendPacket(s.GetTransactOpts(s.key, eth), msgSendPacket)
		s.Require().NoError(err)

		receipt, err := eth.GetTxReciept(ctx, tx.Hash())
		s.Require().NoError(err)
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)

		ethSendTxHash = tx.Hash().Bytes()

		s.True(s.Run("Verify balances on Ethereum", func() {
			userBalance, err := ibcERC20.BalanceOf(nil, ethereumUserAddress)
			s.Require().NoError(err)
			s.Require().Zero(userBalance.Int64())

			// the whole balance should have been burned
			ics20TransferBalance, err := ibcERC20.BalanceOf(nil, ics20Address)
			s.Require().NoError(err)
			s.Require().Zero(ics20TransferBalance.Int64())
		}))
	}))

	s.Require().True(s.Run("Receive packet on SimdB", func() {
		var relayTxBodyBz []byte
		s.Require().True(s.Run("Retrieve relay tx", func() {
			resp, err := s.EthToChainBRelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
				SourceTxIds:     [][]byte{ethSendTxHash},
				TargetChannelId: ibctesting.FirstChannelID,
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Tx)
			s.Require().Empty(resp.Address)

			relayTxBodyBz = resp.Tx
		}))

		s.Require().True(s.Run("Broadcast relay tx", func() {
			_ = s.BroadcastSdkTxBody(ctx, simdB, s.SimdBRelayerSubmitter, 2_000_000, relayTxBodyBz)
		}))

		s.Require().True(s.Run("Verify balances on Cosmos chain", func() {
			finalDenom := transfertypes.NewDenom(
				simdA.Config().Denom,
				transfertypes.NewHop(transfertypes.PortID, ibctesting.FirstChannelID),
				transfertypes.NewHop(transfertypes.PortID, ibctesting.FirstClientID),
			)

			// Check the balance of UserB
			resp, err := e2esuite.GRPCQuery[banktypes.QueryBalanceResponse](ctx, simdB, &banktypes.QueryBalanceRequest{
				Address: simdBUser.FormattedAddress(),
				Denom:   finalDenom.IBCDenom(),
			})
			s.Require().NoError(err)
			s.Require().NotNil(resp.Balance)
			s.Require().Equal(testvalues.TransferAmount, resp.Balance.Amount.Int64())
			s.Require().Equal(finalDenom.IBCDenom(), resp.Balance.Denom)
		}))
	}))
}

func (s *MultichainTestSuite) TestTransferEthToCosmosToCosmos_Groth16() {
	ctx := context.Background()
	proofType := operator.ProofTypeGroth16

	s.SetupSuite(ctx, proofType)

	eth, simdA, simdB := s.EthChain, s.CosmosChains[0], s.CosmosChains[1]

	ics20Address := ethcommon.HexToAddress(s.contractAddresses.Ics20Transfer)
	transferAmount := big.NewInt(testvalues.TransferAmount)
	ethereumUserAddress := crypto.PubkeyToAddress(s.key.PublicKey)
	simdAUser, simdBUser := s.CosmosUsers[0], s.CosmosUsers[1]

	s.Require().True(s.Run("Approve the ICS20Transfer.sol contract to spend the erc20 tokens", func() {
		tx, err := s.erc20Contract.Approve(s.GetTransactOpts(s.key, eth), ics20Address, transferAmount)
		s.Require().NoError(err)

		receipt, err := eth.GetTxReciept(ctx, tx.Hash())
		s.Require().NoError(err)
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)

		allowance, err := s.erc20Contract.Allowance(nil, ethereumUserAddress, ics20Address)
		s.Require().NoError(err)
		s.Require().Equal(transferAmount, allowance)
	}))

	var ethSendTxHash []byte
	s.Require().True(s.Run("Send from Ethereum to SimdA", func() {
		timeout := uint64(time.Now().Add(30 * time.Minute).Unix())

		msgSendPacket := s.createICS20MsgSendPacket(
			ethereumUserAddress,
			s.contractAddresses.Erc20,
			transferAmount,
			simdAUser.FormattedAddress(),
			ibctesting.FirstClientID,
			timeout,
			"",
		)

		tx, err := s.ics26Contract.SendPacket(s.GetTransactOpts(s.key, eth), msgSendPacket)
		s.Require().NoError(err)
		receipt, err := eth.GetTxReciept(ctx, tx.Hash())
		s.Require().NoError(err)
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)

		ethSendTxHash = tx.Hash().Bytes()

		s.True(s.Run("Verify balances on Ethereum", func() {
			// User balance on Ethereum
			userBalance, err := s.erc20Contract.BalanceOf(nil, ethereumUserAddress)
			s.Require().NoError(err)
			s.Require().Equal(new(big.Int).Sub(testvalues.StartingERC20Balance, transferAmount), userBalance)

			// ICS20 contract balance on Ethereum
			escrowBalance, err := s.erc20Contract.BalanceOf(nil, s.escrowContractAddr)
			s.Require().NoError(err)
			s.Require().Equal(transferAmount, escrowBalance)
		}))
	}))

	s.Require().True(s.Run("Receive packets on SimdA", func() {
		var relayTxBodyBz []byte
		s.Require().True(s.Run("Retrieve relay tx", func() {
			resp, err := s.EthToChainARelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
				SourceTxIds:     [][]byte{ethSendTxHash},
				TargetChannelId: ibctesting.FirstChannelID,
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Tx)
			s.Require().Empty(resp.Address)

			relayTxBodyBz = resp.Tx
		}))

		s.Require().True(s.Run("Broadcast relay tx", func() {
			_ = s.BroadcastSdkTxBody(ctx, simdA, s.SimdARelayerSubmitter, 2_000_000, relayTxBodyBz)
			// NOTE: We don't need to check the response since we don't need to acknowledge the packet
		}))

		s.Require().True(s.Run("Verify balances on Cosmos chain", func() {
			denomOnSimdA := transfertypes.NewDenom(s.contractAddresses.Erc20, transfertypes.NewHop(transfertypes.PortID, ibctesting.FirstChannelID))

			// User balance on Cosmos chain
			resp, err := e2esuite.GRPCQuery[banktypes.QueryBalanceResponse](ctx, simdA, &banktypes.QueryBalanceRequest{
				Address: simdAUser.FormattedAddress(),
				Denom:   denomOnSimdA.IBCDenom(),
			})
			s.Require().NoError(err)
			s.Require().NotNil(resp.Balance)
			s.Require().Equal(transferAmount, resp.Balance.Amount.BigInt())
			s.Require().Equal(denomOnSimdA.IBCDenom(), resp.Balance.Denom)
		}))
	}))

	var simdASendTxHash []byte
	s.Require().True(s.Run("Send from SimdA to SimdB", func() {
		denomOnSimdA := transfertypes.NewDenom(s.contractAddresses.Erc20, transfertypes.NewHop(transfertypes.PortID, ibctesting.FirstChannelID))
		timeout := uint64(time.Now().Add(30 * time.Minute).Unix())

		transferPayload := ics20lib.ICS20LibFungibleTokenPacketData{
			// Denom:    denomOnSimdA.IBCDenom(),
			// BUG: Allowing user to choose the above is a bug in ibc-go
			// https://github.com/cosmos/ibc-go/issues/7848
			Denom:    denomOnSimdA.Path(),
			Amount:   transferAmount,
			Sender:   simdAUser.FormattedAddress(),
			Receiver: simdBUser.FormattedAddress(),
			Memo:     "",
		}
		transferBz, err := ics20lib.EncodeFungibleTokenPacketData(transferPayload)
		s.Require().NoError(err)

		payload := channeltypesv2.Payload{
			SourcePort:      transfertypes.PortID,
			DestinationPort: transfertypes.PortID,
			Version:         transfertypes.V1,
			Encoding:        transfertypes.EncodingABI,
			Value:           transferBz,
		}

		resp, err := s.BroadcastMessages(ctx, simdA, simdAUser, 2_000_000, &channeltypesv2.MsgSendPacket{
			SourceChannel:    ibctesting.SecondChannelID,
			TimeoutTimestamp: timeout,
			Payloads: []channeltypesv2.Payload{
				payload,
			},
			Signer: simdAUser.FormattedAddress(),
		})
		s.Require().NoError(err)
		s.Require().NotEmpty(resp.TxHash)

		simdASendTxHash, err = hex.DecodeString(resp.TxHash)
		s.Require().NoError(err)
	}))

	s.Require().True(s.Run("Receive packet on SimdB", func() {
		var txBodyBz []byte
		s.Require().True(s.Run("Retrieve relay tx to SimdB", func() {
			resp, err := s.ChainAToChainBRelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
				SourceTxIds:     [][]byte{simdASendTxHash},
				TargetChannelId: ibctesting.SecondChannelID,
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Tx)
			s.Require().Empty(resp.Address)

			txBodyBz = resp.Tx
		}))

		s.Require().True(s.Run("Broadcast relay tx on SimdB", func() {
			_ = s.BroadcastSdkTxBody(ctx, simdB, s.SimdBRelayerSubmitter, 2_000_000, txBodyBz)
			// NOTE: We don't need to check the response since we don't need to acknowledge the packet
		}))

		s.Require().True(s.Run("Verify balances on Cosmos chain", func() {
			finalDenom := transfertypes.NewDenom(
				s.contractAddresses.Erc20,
				transfertypes.NewHop(transfertypes.PortID, ibctesting.SecondChannelID),
				transfertypes.NewHop(transfertypes.PortID, ibctesting.FirstChannelID),
			)

			// User balance on Cosmos chain
			resp, err := e2esuite.GRPCQuery[banktypes.QueryBalanceResponse](ctx, simdB, &banktypes.QueryBalanceRequest{
				Address: simdBUser.FormattedAddress(),
				Denom:   finalDenom.IBCDenom(),
			})
			s.Require().NoError(err)
			s.Require().NotNil(resp.Balance)
			s.Require().Equal(transferAmount, resp.Balance.Amount.BigInt())
			s.Require().Equal(finalDenom.IBCDenom(), resp.Balance.Denom)
		}))
	}))
}

func (s *MultichainTestSuite) createICS20MsgSendPacket(
	sender ethcommon.Address,
	denom string,
	amount *big.Int,
	receiver string,
	sourceChannel string,
	timeoutTimestamp uint64,
	memo string,
) ics26router.IICS26RouterMsgsMsgSendPacket {
	msgSendTransfer := ics20transfer.IICS20TransferMsgsSendTransferMsg{
		Denom:            denom,
		Amount:           amount,
		Receiver:         receiver,
		SourceChannel:    sourceChannel,
		DestPort:         transfertypes.PortID,
		TimeoutTimestamp: timeoutTimestamp,
		Memo:             memo,
	}
	msgSendPacket, err := s.ics20Contract.ContractCaller.NewMsgSendPacketV1(nil, sender, msgSendTransfer)
	s.Require().NoError(err)

	// Because of the way abi generation work, the type returned by ics20 is ics20transfer.IICS26RouterMsgsMsgSendPacket
	// So we just move the values over here:
	return ics26router.IICS26RouterMsgsMsgSendPacket{
		SourceChannel:    sourceChannel,
		TimeoutTimestamp: timeoutTimestamp,
		Payloads: []ics26router.IICS26RouterMsgsPayload{
			{
				SourcePort: msgSendPacket.Payloads[0].SourcePort,
				DestPort:   msgSendPacket.Payloads[0].DestPort,
				Version:    msgSendPacket.Payloads[0].Version,
				Encoding:   msgSendPacket.Payloads[0].Encoding,
				Value:      msgSendPacket.Payloads[0].Value,
			},
		},
	}
}
