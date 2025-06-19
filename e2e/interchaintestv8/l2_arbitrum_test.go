package main

import (
	"context"
	"fmt"
	"os"
	"testing"

	"github.com/stretchr/testify/suite"

	"github.com/ethereum/go-ethereum/ethclient"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/chainconfig"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
)

// L2ArbitrumTestSuite is a struct that holds the test suite for Arbitrum L2 chain.
type L2ArbitrumTestSuite struct {
	suite.Suite

	arbitrumChain chainconfig.TestnodeArbitrumChain
}

// TestWithL2ArbitrumTestSuite is the boilerplate code that allows the test suite to be run
func TestWithL2ArbitrumTestSuite(t *testing.T) {
	suite.Run(t, new(L2ArbitrumTestSuite))
}

// SetupSuite calls the underlying L2ArbitrumTestSuite's SetupSuite method
// and sets up the Arbitrum testnode
func (s *L2ArbitrumTestSuite) SetupSuite(ctx context.Context) {
	chainconfig.DefaultChainSpecs = append(chainconfig.DefaultChainSpecs, chainconfig.IbcGoChainSpec("ibc-go-simd-2", "simd-2"))

	os.Setenv(testvalues.EnvKeyEthTestnetType, testvalues.EthTestnetTypeNone)

	// TODO: This should happen through the top level test suite ideally, not here
	arbitrumChain, err := chainconfig.SpinUpTestnodeArbitrum(ctx)
	s.Require().NoError(err)
	s.arbitrumChain = arbitrumChain
	fmt.Printf("Arbitrum Testnode Chain: %+v\n", arbitrumChain)

	s.T().Cleanup(func() {
		s.arbitrumChain.Destroy(ctx)
	})
}

func (s *L2ArbitrumTestSuite) TestDeployment() {
	s.T().Log("Running L2 test suite with Arbitrum testnode")

	ctx := context.Background()
	s.SetupSuite(ctx)

	s.Require().NotEmpty(s.arbitrumChain.ExecutionRPC, "Arbitrum ExecutionRPC should not be empty")
	s.Require().NotEmpty(s.arbitrumChain.ConsensusRPC, "Arbitrum ConsensusRPC should not be empty")

	// Test connection to the Arbitrum RPC
	client, err := ethclient.Dial(s.arbitrumChain.ExecutionRPC)
	s.Require().NoError(err)

	// Get the latest block number
	blockNumber, err := client.BlockNumber(ctx)
	s.Require().NoError(err)
	s.T().Logf("Latest block number: %d", blockNumber)

	// Get chain ID
	chainID, err := client.ChainID(ctx)
	s.Require().NoError(err)
	s.T().Logf("Chain ID: %s", chainID.String())

	// Test that we can get account balance (should work even with 0x0 address)
	balance, err := client.BalanceAt(ctx, [20]byte{}, nil)
	s.Require().NoError(err)
	s.T().Logf("Zero address balance: %s", balance.String())
}
