package main

import (
	"context"
	"os"
	"testing"

	"github.com/stretchr/testify/suite"

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
	//nolint:govet
	ibcSpecs2 := *chainconfig.DefaultChainSpecs[0]
	ibcSpecs2.ChainID = "simd-2"
	ibcSpecs2.Name = "ibc-go-simd-2"

	chainconfig.DefaultChainSpecs = append(chainconfig.DefaultChainSpecs, &ibcSpecs2)

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
			_ = relayerProcess.Kill()
		}
	})

	s.Require().True(s.Run("Create Relayer Client", func() {
		var err error
		s.AtoBRelayerClient, err = relayer.GetGRPCClient(configInfo.ChainAToChainBGRPCAddress())
		s.Require().NoError(err)

		s.BtoARelayerClient, err = relayer.GetGRPCClient(configInfo.ChainBToChainAGRPCAddress())
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
