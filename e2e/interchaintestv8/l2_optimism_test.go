package main

import (
	"context"
	"fmt"
	"os"
	"testing"

	"github.com/ethereum-optimism/optimism/op-service/client"
	"github.com/ethereum-optimism/optimism/op-service/sources"
	"github.com/stretchr/testify/suite"

	"github.com/ethereum/go-ethereum/ethclient"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/chainconfig"
	// "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/ethereum"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
)

// L2OptimismTestSuite is a struct that holds the test suite for Optimism L2 chain.
type L2OptimismTestSuite struct {
	suite.Suite

	kurtosisOptimismChain chainconfig.KurtosisOptimismChain
}

// TestWithL2OptimismTestSuite is the boilerplate code that allows the test suite to be run
func TestWithL2OptimismTestSuite(t *testing.T) {
	suite.Run(t, new(L2OptimismTestSuite))
}

// SetupSuite calls the underlying L2OptimismTestSuite's SetupSuite method
// and deploys the IbcEureka contract
func (s *L2OptimismTestSuite) SetupSuite(ctx context.Context) {
	chainconfig.DefaultChainSpecs = append(chainconfig.DefaultChainSpecs, chainconfig.IbcGoChainSpec("ibc-go-simd-2", "simd-2"))

	os.Setenv(testvalues.EnvKeyEthTestnetType, testvalues.EthTestnetTypeNone)

	// TODO: This should happen through the top level test suite ideally, not here
	kurtosisOptimismChain, err := chainconfig.SpinUpKurtosisOptimism(ctx)
	s.Require().NoError(err)
	s.kurtosisOptimismChain = kurtosisOptimismChain
	fmt.Printf("Kurtosis Optimism Chain: %+v\n", kurtosisOptimismChain)

	s.T().Cleanup(func() {
		s.kurtosisOptimismChain.Destroy(ctx)
	})

	// s.EthChain, err = ethereum.NewEthereum(ctx, kurtosisOptimismChain.ExecutionRPC, nil, kurtosisOptimismChain.Faucet)
	// s.Require().NoError(err)
}

func (s *L2OptimismTestSuite) TestDeployment() {
	s.T().Log("Running L2 test suite with Kurtosis Optimism chain")

	ctx := context.Background()
	s.SetupSuite(ctx)

	// s.Require().NotEmpty(s.EthChain.RPC, "Ethereum RPC should not be empty")

	consensusClient, err := ethclient.Dial(s.kurtosisOptimismChain.ConsensusRPC)
	s.Require().NoError(err)
	baseClient := client.NewBaseRPCClient(consensusClient.Client())
	rollupClient := sources.NewRollupClient(baseClient)

	rollupConfig, err := rollupClient.RollupConfig(ctx)
	s.Require().NoError(err)

	s.T().Logf("Rollup config: %+v", rollupConfig)

	// logger := log.New(ctx)
	//
	// clientConfig := sources.L2ClientDefaultConfig()
	//
	// sources.NewL2Client(baseClient, logger, nil, nil)
}
