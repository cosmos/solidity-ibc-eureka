package main

import (
	"context"
	"encoding/hex"
	"os"
	"testing"

	"github.com/stretchr/testify/suite"

	"github.com/ethereum/go-ethereum/crypto"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/operator"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/relayer"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
	relayertypes "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/relayer"
)

// RelayerTestSuite is a suite of tests that wraps IbcEurekaTestSuite
// and can provide additional functionality
type RelayerTestSuite struct {
	IbcEurekaTestSuite

	RelayerClient relayertypes.RelayerServiceClient
}

// TestWithIbcEurekaTestSuite is the boilerplate code that allows the test suite to be run
func TestWithRelayerTestSuite(t *testing.T) {
	suite.Run(t, new(RelayerTestSuite))
}

// SetupSuite is called once, before the start of the test suite
func (s *RelayerTestSuite) SetupSuite(ctx context.Context, proofType operator.SupportedProofType) {
	s.IbcEurekaTestSuite.SetupSuite(ctx, proofType)

	eth, simd := s.ChainA, s.ChainB

	var relayerProcess *os.Process
	s.Require().True(s.Run("Start Relayer", func() {
		relayerKey, err := eth.CreateAndFundUser()
		s.Require().NoError(err)

		configInfo := relayer.ConfigInfo{
			TmRPC:         simd.GetHostRPCAddress(),
			ICS26Address:  s.contractAddresses.Ics26Router,
			EthRPC:        eth.RPC,
			PrivateKey:    hex.EncodeToString(crypto.FromECDSA(relayerKey)),
			ProofType:     proofType.String(),
			SP1PrivateKey: os.Getenv(testvalues.EnvKeySp1PrivateKey),
		}

		err = configInfo.GenerateConfigFile("relayer_config.json")
		s.Require().NoError(err)

		relayerProcess, err = relayer.StartRelayer("relayer_config.json")
		s.Require().NoError(err)

		s.T().Cleanup(func() {
			os.Remove("relayer_config.json")
		})
	}))

	s.T().Cleanup(func() {
		if relayerProcess != nil {
			_ = relayerProcess.Kill()
		}
	})

	s.Require().True(s.Run("Create Relayer Client", func() {
		var err error
		s.RelayerClient, err = relayer.GetGRPCClient()
		s.Require().NoError(err)
	}))
}

// TestRelayer is a test that runs the relayer
func (s *RelayerTestSuite) TestRelayerInfo() {
	ctx := context.Background()
	s.SetupSuite(ctx, operator.ProofTypeGroth16)

	eth, simd := s.ChainA, s.ChainB

	s.Run("Relayer Info", func() {
		info, err := s.RelayerClient.Info(context.Background(), &relayertypes.InfoRequest{})
		s.Require().NoError(err)
		s.Require().NotNil(info)

		s.T().Logf("Relayer Info: %+v", info)

		s.Require().Equal(simd.Config().ChainID, info.SourceChain.ChainId)
		s.Require().Equal(eth.ChainID.String(), info.TargetChain.ChainId)
	})
}
