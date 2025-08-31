package main

import (
	"context"
	"testing"

	gmptypes "github.com/cosmos/ibc-go/v10/modules/apps/27-gmp/types"
	"github.com/cosmos/solidity-ibc-eureka/packages/go-abigen/ibcxerc20"
	"github.com/cosmos/solidity-ibc-eureka/packages/go-abigen/ics27gmp"
	ethcommon "github.com/ethereum/go-ethereum/common"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/e2esuite"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types"
	"github.com/stretchr/testify/suite"
)

// DemoTestSuite is a suite of tests that wraps DemoTestSuite
// and can provide additional functionality
type DemoTestSuite struct {
	IbcEurekaGmpTestSuite

	ibcXERC20 *ibcxerc20.Contract
}

// TestWithDemoTestSuite is the boilerplate code that allows the test suite to be run
func TestWithDemoTestSuite(t *testing.T) {
	suite.Run(t, new(RelayerTestSuite))
}

func (s *DemoTestSuite) SetupSuite(ctx context.Context, proofType types.SupportedProofType) {
	s.IbcEurekaGmpTestSuite.SetupSuite(ctx, proofType)

	eth, simd := s.EthChain, s.CosmosChains[0]

	s.Require().True(s.Run("IBCXERC20 Setup", func() {
		var err error
		s.ibcXERC20, err = ibcxerc20.NewContract(ethcommon.HexToAddress(s.contractAddresses.IbcXErc20), eth.RPCClient)
		s.Require().NoError(err)

		s.Require().True(s.Run("Set the Cosmos account in the IBCXERC20 contract", func() {
			resp, err := e2esuite.GRPCQuery[gmptypes.QueryAccountAddressResponse](ctx, simd, &gmptypes.QueryAccountAddressRequest{
				ClientId: testvalues.FirstWasmClientID,
				Sender:   s.contractAddresses.IbcXErc20,
				Salt:     "",
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.AccountAddress)

			_, err = s.ibcXERC20.SetCosmosAccount(s.GetTransactOpts(s.deployer, eth), resp.AccountAddress)
			s.Require().NoError(err)
		}))

		s.Require().True(s.Run("Set bridge contract in the IBCXERC20 contract", func() {
			moduleAcc, err := simd.AuthQueryModuleAccount(ctx, "tokenfactory")
			s.Require().NoError(err)
			s.Require().NotEmpty(moduleAcc.Address)

			bridgeAddr, err := s.ics27Contract.GetOrComputeAccountAddress(nil, ics27gmp.IICS27GMPMsgsAccountIdentifier{
				ClientId: testvalues.CustomClientID,
				Sender:   moduleAcc.Address,
				Salt:     []byte(""),
			})
			s.Require().NoError(err)

			_, err = s.ibcXERC20.SetBridge(s.GetTransactOpts(s.deployer, eth), bridgeAddr)
			s.Require().NoError(err)
		}))
	}))
}

func (s *DemoTestSuite) Test_Deploy() {
	ctx := context.Background()
	proofType := types.GetEnvProofType()
	s.DeployTest(ctx, proofType)
}

func (s *DemoTestSuite) DeployTest(ctx context.Context, proofType types.SupportedProofType) {
	s.SetupSuite(ctx, proofType)
}
