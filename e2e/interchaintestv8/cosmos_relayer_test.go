package main

import (
	"context"
	"os"
	"testing"

	"github.com/stretchr/testify/suite"

	codectypes "github.com/cosmos/cosmos-sdk/codec/types"

	clienttypes "github.com/cosmos/ibc-go/v9/modules/core/02-client/types"
	commitmenttypes "github.com/cosmos/ibc-go/v9/modules/core/23-commitment/types"
	ibctm "github.com/cosmos/ibc-go/v9/modules/light-clients/07-tendermint"
	ibctesting "github.com/cosmos/ibc-go/v9/testing"

	"github.com/strangelove-ventures/interchaintest/v8/chain/cosmos"
	"github.com/strangelove-ventures/interchaintest/v8/ibc"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/chainconfig"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/e2esuite"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/relayer"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
	relayertypes "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/relayer"
)

// CosmosRelayerTestSuite is a struct that holds the test suite for two Cosmos chains.
type CosmosRelayerTestSuite struct {
	e2esuite.TestSuite

	SimdA *cosmos.CosmosChain
	SimdB *cosmos.CosmosChain

	SimdASubmitter ibc.Wallet
	SimdBSubmitter ibc.Wallet

	AtoBRelayerClient relayertypes.RelayerServiceClient
	BtoARelayerClient relayertypes.RelayerServiceClient
}

// TestWithIbcEurekaTestSuite is the boilerplate code that allows the test suite to be run
func TestWithCosmosRelayerTestSuite(t *testing.T) {
	suite.Run(t, new(CosmosRelayerTestSuite))
}

// SetupSuite calls the underlying IbcEurekaTestSuite's SetupSuite method
// and deploys the IbcEureka contract
func (s *CosmosRelayerTestSuite) SetupSuite(ctx context.Context) {
	chainconfig.DefaultChainSpecs = append(chainconfig.DefaultChainSpecs, chainconfig.IbcGoChainSpec("ibc-go-simd-2", "simd-2"))

	os.Setenv(testvalues.EnvKeyEthTestnetType, testvalues.EthTestnetTypeNone)

	s.TestSuite.SetupSuite(ctx)

	s.SimdA, s.SimdB = s.CosmosChains[0], s.CosmosChains[1]
	s.SimdASubmitter = s.CreateAndFundCosmosUser(ctx, s.SimdA)
	s.SimdBSubmitter = s.CreateAndFundCosmosUser(ctx, s.SimdB)

	var relayerProcess *os.Process
	var configInfo relayer.CosmosToCosmosConfigInfo
	s.Require().True(s.Run("Start Relayer", func() {
		err := os.Chdir("../..")
		s.Require().NoError(err)

		configInfo = relayer.CosmosToCosmosConfigInfo{
			ChainATmRPC: s.SimdA.GetHostRPCAddress(),
			ChainBTmRPC: s.SimdB.GetHostRPCAddress(),
			ChainAUser:  s.SimdASubmitter.FormattedAddress(),
			ChainBUser:  s.SimdBSubmitter.FormattedAddress(),
		}

		err = configInfo.GenerateCosmosToCosmosConfigFile(testvalues.RelayerConfigFilePath)
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
		s.AtoBRelayerClient, err = relayer.GetGRPCClient(configInfo.ChainAToChainBGRPCAddress())
		s.Require().NoError(err)

		s.BtoARelayerClient, err = relayer.GetGRPCClient(configInfo.ChainBToChainAGRPCAddress())
		s.Require().NoError(err)
	}))

	s.Require().True(s.Run("Create Light Client of Chain A on Chain B", func() {
		simdAHeader, err := s.FetchCosmosHeader(ctx, s.SimdA)
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

		_, err = s.BroadcastMessages(ctx, s.SimdB, s.SimdBSubmitter, 200_000, &clienttypes.MsgCreateClient{
			ClientState:    clientStateAny,
			ConsensusState: consensusStateAny,
			Signer:         s.SimdBSubmitter.FormattedAddress(),
		})
		s.Require().NoError(err)
	}))

	s.Require().True(s.Run("Create Light Client of Chain B on Chain A", func() {
		simdBHeader, err := s.FetchCosmosHeader(ctx, s.SimdB)
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

		_, err = s.BroadcastMessages(ctx, s.SimdA, s.SimdASubmitter, 200_000, &clienttypes.MsgCreateClient{
			ClientState:    clientStateAny,
			ConsensusState: consensusStateAny,
			Signer:         s.SimdASubmitter.FormattedAddress(),
		})
		s.Require().NoError(err)
	}))
}

// TestRelayer is a test that runs the relayer
func (s *CosmosRelayerTestSuite) TestRelayerInfo() {
	ctx := context.Background()
	s.SetupSuite(ctx)

	s.Run("Chain A to B Relayer Info", func() {
		info, err := s.AtoBRelayerClient.Info(context.Background(), &relayertypes.InfoRequest{})
		s.Require().NoError(err)
		s.Require().NotNil(info)

		s.T().Logf("Relayer Info: %+v", info)

		s.Require().Equal(s.SimdA.Config().ChainID, info.SourceChain.ChainId)
		s.Require().Equal(s.SimdB.Config().ChainID, info.TargetChain.ChainId)
	})

	s.Run("Chain B to A Relayer Info", func() {
		info, err := s.BtoARelayerClient.Info(context.Background(), &relayertypes.InfoRequest{})
		s.Require().NoError(err)
		s.Require().NotNil(info)

		s.T().Logf("Relayer Info: %+v", info)

		s.Require().Equal(s.SimdB.Config().ChainID, info.SourceChain.ChainId)
		s.Require().Equal(s.SimdA.Config().ChainID, info.TargetChain.ChainId)
	})
}
