package main

import (
	"context"
	"os"
	"testing"

	"github.com/stretchr/testify/suite"

	"github.com/gagliardetto/solana-go"
	"github.com/gagliardetto/solana-go/rpc"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/e2esuite"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
)

type IbcEurekaSolanaTestSuite struct {
	e2esuite.TestSuite

	SolanaUser *solana.Wallet
}

// TestWithIbcEurekaTestSuite is the boilerplate code that allows the test suite to be run
func TestWithIbcEurekaSolanaTestSuite(t *testing.T) {
	suite.Run(t, new(IbcEurekaSolanaTestSuite))
}

// SetupSuite calls the underlying IbcEurekaTestSuite's SetupSuite method
// and deploys the IbcEureka contract
func (s *IbcEurekaSolanaTestSuite) SetupSuite(ctx context.Context) {
	os.Setenv(testvalues.EnvKeyEthTestnetType, testvalues.EthTestnetTypeNone)
	os.Setenv(testvalues.EnvKeySolanaTestnetType, testvalues.SolanaTestnetType_Localnet)
	s.TestSuite.SetupSuite(ctx)

	var err error
	s.SolanaUser, err = s.SolanaChain.CreateAndFundWallet()
	s.Require().NoError(err)
}

func (s *IbcEurekaSolanaTestSuite) Test_Deploy() {
	ctx := context.Background()
	s.SetupSuite(ctx)

	res, err := s.SolanaChain.RPCClient.GetBalance(ctx, s.SolanaUser.PublicKey(), rpc.CommitmentConfirmed) // Ensure the user has a balance
	s.Require().NoError(err)
	s.Require().Equal(res.Value, testvalues.InitialSolBalance)
}
