package main

import (
	"context"
	"testing"

	"github.com/stretchr/testify/suite"

	"github.com/gagliardetto/solana-go"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/e2esuite"
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
	s.TestSuite.SetupSuite(ctx)

	s.SolanaUser = solana.NewWallet()
}
