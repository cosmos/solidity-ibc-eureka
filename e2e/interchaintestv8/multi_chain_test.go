package main

import (
	"context"
	"crypto/ecdsa"
	"encoding/hex"
	"math/big"
	"os"
	"strconv"
	"testing"

	"github.com/cosmos/solidity-ibc-eureka/abigen/ibcstore"
	"github.com/cosmos/solidity-ibc-eureka/abigen/ics20transfer"
	"github.com/cosmos/solidity-ibc-eureka/abigen/ics26router"
	"github.com/cosmos/solidity-ibc-eureka/abigen/icscore"
	"github.com/cosmos/solidity-ibc-eureka/abigen/sp1ics07tendermint"
	"github.com/stretchr/testify/suite"

	ethcommon "github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/crypto"

	channeltypesv2 "github.com/cosmos/ibc-go/v9/modules/core/04-channel/v2/types"
	commitmenttypesv2 "github.com/cosmos/ibc-go/v9/modules/core/23-commitment/types/v2"
	ibcexported "github.com/cosmos/ibc-go/v9/modules/core/exported"
	ibctesting "github.com/cosmos/ibc-go/v9/testing"

	"github.com/strangelove-ventures/interchaintest/v9/ibc"

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

	contractAddresses ethereum.DeployedContracts

	simdASP1Ics07Contract *sp1ics07tendermint.Contract
	simdBSP1Ics07Contract *sp1ics07tendermint.Contract
	icsCoreContract       *icscore.Contract
	ics26Contract         *ics26router.Contract
	ics20Contract         *ics20transfer.Contract
	erc20Contract         *erc20.Contract
	ibcStoreContract      *ibcstore.Contract
	escrowContractAddr    ethcommon.Address

	EthToChainARelayerClient relayertypes.RelayerServiceClient
	ChainAToEthRelayerClient relayertypes.RelayerServiceClient
	EthToChainBRelayerClient relayertypes.RelayerServiceClient
	ChainBToEthRelayerClient relayertypes.RelayerServiceClient

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
			s.FailNow("Mock prover not supported")
		case testvalues.EnvValueSp1Prover_Network:
			// make sure that the SP1_PRIVATE_KEY is set.
			s.Require().NotEmpty(os.Getenv(testvalues.EnvKeySp1PrivateKey))

			stdout, err = eth.ForgeScript(s.deployer, testvalues.E2EDeployScriptPath)
			s.Require().NoError(err)
		default:
			s.Require().Fail("invalid prover type: %s", prover)
		}

		s.contractAddresses, err = ethereum.GetEthContractsFromDeployOutput(string(stdout))
		s.Require().NoError(err)
		s.simdASP1Ics07Contract, err = sp1ics07tendermint.NewContract(ethcommon.HexToAddress(s.contractAddresses.Ics07Tendermint), eth.RPCClient)
		s.Require().NoError(err)
		s.icsCoreContract, err = icscore.NewContract(ethcommon.HexToAddress(s.contractAddresses.IcsCore), eth.RPCClient)
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

	var simdBSp1Ics07ContractAddress string
	s.Require().True(s.Run("Deploy SimdB light client on ethereum", func() {
		os.Setenv(testvalues.EnvKeyTendermintRPC, simdB.GetHostRPCAddress())

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
			s.FailNow("Mock prover not supported")
		case testvalues.EnvValueSp1Prover_Network:
			// make sure that the SP1_PRIVATE_KEY is set.
			s.Require().NotEmpty(os.Getenv(testvalues.EnvKeySp1PrivateKey))

			stdout, err = eth.ForgeScript(s.deployer, testvalues.SP1ICS07DeployScriptPath, "--json")
			s.Require().NoError(err)
		default:
			s.Require().Fail("invalid prover type: %s", prover)
		}

		simdBSp1Ics07ContractAddress, err = ethereum.GetOnlySp1Ics07AddressFromStdout(string(stdout))
		s.Require().NoError(err)
		s.Require().NotEmpty(simdBSp1Ics07ContractAddress)
		s.Require().True(ethcommon.IsHexAddress(simdBSp1Ics07ContractAddress))

		s.simdBSP1Ics07Contract, err = sp1ics07tendermint.NewContract(ethcommon.HexToAddress(simdBSp1Ics07ContractAddress), eth.RPCClient)
		s.Require().NoError(err)
	}))

	s.T().Cleanup(func() {
		_ = os.Remove(testvalues.Sp1GenesisFilePath)
	})

	s.Require().True(s.Run("Fund address with ERC20", func() {
		tx, err := s.erc20Contract.Transfer(s.GetTransactOpts(eth.Faucet, eth), crypto.PubkeyToAddress(s.key.PublicKey), big.NewInt(testvalues.InitialBalance))
		s.Require().NoError(err)

		_, err = eth.GetTxReciept(ctx, tx.Hash()) // wait for the tx to be mined
		s.Require().NoError(err)
	}))

	s.Require().True(s.Run("Add ethereum light client on SimdA", func() {
		s.CreateEthereumLightClient(ctx, simdA, s.SimdARelayerSubmitter, s.contractAddresses.IbcStore)
	}))

	s.Require().True(s.Run("Add simdA client and counterparty on EVM", func() {
		channel := icscore.IICS04ChannelMsgsChannel{
			CounterpartyId: ibctesting.FirstChannelID,
			MerklePrefix:   [][]byte{[]byte(ibcexported.StoreKey), []byte("")},
		}
		lightClientAddress := ethcommon.HexToAddress(s.contractAddresses.Ics07Tendermint)
		tx, err := s.icsCoreContract.AddChannel(s.GetTransactOpts(s.key, eth), ibcexported.Tendermint, channel, lightClientAddress)
		s.Require().NoError(err)

		receipt, err := eth.GetTxReciept(ctx, tx.Hash())
		s.Require().NoError(err)

		event, err := e2esuite.GetEvmEvent(receipt, s.icsCoreContract.ParseICS04ChannelAdded)
		s.Require().NoError(err)
		s.Require().Equal(ibctesting.FirstClientID, event.ChannelId)
		s.Require().Equal(ibctesting.FirstChannelID, event.Channel.CounterpartyId)
		s.TendermintLightClientID = event.ChannelId
	}))

	s.Require().True(s.Run("Add ethereum light client on SimdB", func() {
		s.CreateEthereumLightClient(ctx, simdB, s.SimdBRelayerSubmitter, s.contractAddresses.IbcStore)
	}))

	s.Require().True(s.Run("Add simdB client and counterparty on EVM", func() {
		channel := icscore.IICS04ChannelMsgsChannel{
			CounterpartyId: ibctesting.FirstChannelID,
			MerklePrefix:   [][]byte{[]byte(ibcexported.StoreKey), []byte("")},
		}
		lightClientAddress := ethcommon.HexToAddress(simdBSp1Ics07ContractAddress)
		tx, err := s.icsCoreContract.AddChannel(s.GetTransactOpts(s.key, eth), ibcexported.Tendermint, channel, lightClientAddress)
		s.Require().NoError(err)

		receipt, err := eth.GetTxReciept(ctx, tx.Hash())
		s.Require().NoError(err)

		event, err := e2esuite.GetEvmEvent(receipt, s.icsCoreContract.ParseICS04ChannelAdded)
		s.Require().NoError(err)
		s.Require().Equal(ibctesting.SecondClientID, event.ChannelId)
		s.Require().Equal(ibctesting.FirstChannelID, event.Channel.CounterpartyId)
		s.TendermintLightClientID = event.ChannelId
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
			ChainATmRPC:         simdA.GetHostRPCAddress(),
			ChainASignerAddress: s.SimdARelayerSubmitter.FormattedAddress(),
			EthToChainBPort:     3002,
			ChainBToEthPort:     3003,
			ChainBTmRPC:         simdB.GetHostRPCAddress(),
			ChainBSignerAddress: s.SimdBRelayerSubmitter.FormattedAddress(),
			ICS26Address:        s.contractAddresses.Ics26Router,
			EthRPC:              eth.RPC,
			BeaconAPI:           beaconAPI,
			SP1PrivateKey:       os.Getenv(testvalues.EnvKeySp1PrivateKey),
			Mock:                os.Getenv(testvalues.EnvKeyEthTestnetType) == testvalues.EthTestnetTypePoW,
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
	}))
}

func (s *MultichainTestSuite) TestDeploy_Groth16() {
	ctx := context.Background()
	proofType := operator.ProofTypeGroth16

	s.SetupSuite(ctx, proofType)

	_, simdA, simdB := s.EthChain, s.CosmosChains[0], s.CosmosChains[1]

	s.Require().True(s.Run("Verify SimdA SP1 Client", func() {
		clientState, err := s.simdASP1Ics07Contract.GetClientState(nil)
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
		clientState, err := s.simdBSP1Ics07Contract.GetClientState(nil)
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
}
