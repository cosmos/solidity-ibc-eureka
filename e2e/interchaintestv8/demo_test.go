package main

import (
	"context"
	"encoding/hex"
	"fmt"
	"testing"

	gmptypes "github.com/cosmos/ibc-go/v10/modules/apps/27-gmp/types"
	"github.com/cosmos/solidity-ibc-eureka/packages/go-abigen/ibcxerc20"
	"github.com/cosmos/solidity-ibc-eureka/packages/go-abigen/ics27gmp"
	factorytypes "github.com/cosmos/wfchain/x/tokenfactory/types"
	ethcommon "github.com/ethereum/go-ethereum/common"
	ethtypes "github.com/ethereum/go-ethereum/core/types"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/e2esuite"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types"
	relayertypes "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/relayer"
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
	suite.Run(t, new(DemoTestSuite))
}

func (s *DemoTestSuite) SetupSuite(ctx context.Context, proofType types.SupportedProofType) {
	s.IbcEurekaGmpTestSuite.SetupSuite(ctx, proofType)

	eth, simd := s.EthChain, s.CosmosChains[0]

	ics26Address := ethcommon.HexToAddress(s.contractAddresses.Ics26Router)

	s.Require().True(s.Run("IBCXERC20 Setup", func() {
		var err error
		s.ibcXERC20, err = ibcxerc20.NewContract(ethcommon.HexToAddress(s.contractAddresses.IbcXErc20), eth.RPCClient)
		s.Require().NoError(err)

		_, err = s.ibcXERC20.SetClientId(s.GetTransactOpts(s.deployer, eth), testvalues.CustomClientID)
		s.Require().NoError(err)

		s.Require().True(s.Run("Set the Cosmos account in the IBCXERC20 contract", func() {
			resp, err := e2esuite.GRPCQuery[gmptypes.QueryAccountAddressResponse](ctx, simd, &gmptypes.QueryAccountAddressRequest{
				ClientId: testvalues.FirstWasmClientID,
				Sender:   s.contractAddresses.IbcXErc20,
				Salt:     "",
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.AccountAddress)

			s.T().Logf("IBC-XERC20 Cosmos account: %s", resp.AccountAddress)

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

	s.Require().True(s.Run("TokenFactory Setup", func() {
		s.Require().True(s.Run("Create a new denom", func() {
			tx, err := s.ibcXERC20.CreateDenom(s.GetTransactOpts(s.deployer, eth))
			s.Require().NoError(err)

			receipt, err := eth.GetTxReciept(ctx, tx.Hash())
			s.Require().NoError(err)
			s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status, fmt.Sprintf("Tx failed: %+v", receipt))
			ethSendTxHash := tx.Hash().Bytes()

			var ackTxHash []byte
			s.Require().True(s.Run("Receive packets on Cosmos chain", func() {
				var relayTxBodyBz []byte
				s.Require().True(s.Run("Retrieve relay tx", func() {
					resp, err := s.RelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
						SrcChain:    eth.ChainID.String(),
						DstChain:    simd.Config().ChainID,
						SourceTxIds: [][]byte{ethSendTxHash},
						SrcClientId: testvalues.CustomClientID,
						DstClientId: testvalues.FirstWasmClientID,
					})
					s.Require().NoError(err)
					s.Require().NotEmpty(resp.Tx)
					s.Require().Empty(resp.Address)

					relayTxBodyBz = resp.Tx
				}))

				s.Require().True(s.Run("Broadcast relay tx", func() {
					resp := s.MustBroadcastSdkTxBody(ctx, simd, s.SimdRelayerSubmitter, 10_000_000, relayTxBodyBz)

					ackTxHash, err = hex.DecodeString(resp.TxHash)
					s.Require().NoError(err)
					s.Require().NotEmpty(ackTxHash)
				}))
				// s.Require().True(s.Run("Verify denom on Cosmos chain", func() {
				// }))
			}))

			s.Require().True(s.Run("Acknowledge packets on Ethereum", func() {
				var ackRelayTx []byte
				s.Require().True(s.Run("Retrieve relay tx", func() {
					resp, err := s.RelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
						SrcChain:    simd.Config().ChainID,
						DstChain:    eth.ChainID.String(),
						SourceTxIds: [][]byte{ackTxHash},
						SrcClientId: testvalues.FirstWasmClientID,
						DstClientId: testvalues.CustomClientID,
					})
					s.Require().NoError(err)
					s.Require().NotEmpty(resp.Tx)
					s.Require().Equal(resp.Address, ics26Address.String())

					ackRelayTx = resp.Tx
				}))

				s.Require().True(s.Run("Submit relay tx", func() {
					receipt, err := eth.BroadcastTx(ctx, s.EthRelayerSubmitter, 10_000_000, &ics26Address, ackRelayTx)
					s.Require().NoError(err)
					s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status, fmt.Sprintf("Tx failed: %+v", receipt))

					// Verify the ack packet event exists
					_, err = e2esuite.GetEvmEvent(receipt, s.ics26Contract.ParseAckPacket)
					s.Require().NoError(err)
				}))
			}))
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
