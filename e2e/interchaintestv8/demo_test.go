package main

import (
	"context"
	"testing"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types"
	"github.com/stretchr/testify/suite"
)

// DemoTestSuite is a suite of tests that wraps DemoTestSuite
// and can provide additional functionality
type DemoTestSuite struct {
	IbcEurekaGmpTestSuite
}

// TestWithDemoTestSuite is the boilerplate code that allows the test suite to be run
func TestWithDemoTestSuite(t *testing.T) {
	suite.Run(t, new(RelayerTestSuite))
}

func (s *DemoTestSuite) SetupSuite(ctx context.Context, proofType types.SupportedProofType) {
	s.IbcEurekaGmpTestSuite.SetupSuite(ctx, proofType)
}
