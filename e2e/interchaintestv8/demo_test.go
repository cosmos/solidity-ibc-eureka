package main

import (
	"context"
	"encoding/hex"
	"fmt"
	"math/big"
	"testing"
	"time"

	"github.com/stretchr/testify/suite"

	ethcommon "github.com/ethereum/go-ethereum/common"
	ethtypes "github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/crypto"

	sdkmath "cosmossdk.io/math"

	sdk "github.com/cosmos/cosmos-sdk/types"
	banktypes "github.com/cosmos/cosmos-sdk/x/bank/types"

	gmptypes "github.com/cosmos/ibc-go/v10/modules/apps/27-gmp/types"

	"github.com/cosmos/solidity-ibc-eureka/packages/go-abigen/ibcxerc20"
	"github.com/cosmos/solidity-ibc-eureka/packages/go-abigen/ics27gmp"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/e2esuite"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types"
	relayertypes "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/relayer"
	wfchain "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/wfchain"
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

	simdUser := s.CosmosUsers[0]

	ibcxerc20Address := ethcommon.HexToAddress(s.contractAddresses.IbcXErc20)

	s.Require().True(s.Run("IBCXERC20 Setup", func() {
		var err error
		s.ibcXERC20, err = ibcxerc20.NewContract(ibcxerc20Address, eth.RPCClient)
		s.Require().NoError(err)

		_, err = s.ibcXERC20.SetClientId(s.GetTransactOpts(s.deployer, eth), testvalues.CustomClientID)
		s.Require().NoError(err)

		s.Require().True(s.Run("Set the Cosmos account in the IBCXERC20 contract", func() {
			resp, err := e2esuite.GRPCQuery[gmptypes.QueryAccountAddressResponse](ctx, simd, &gmptypes.QueryAccountAddressRequest{
				ClientId: testvalues.FirstWasmClientID,
				Sender:   ibcxerc20Address.String(),
				Salt:     "",
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.AccountAddress)

			s.T().Logf("IBC-XERC20 Cosmos account: %s", resp.AccountAddress)
			s.T().Logf("IBC-XERC20 Ethereum account: %s", ibcxerc20Address.String())

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
		createDenomMsg := &wfchain.MsgCreateDenom{
			Sender: simdUser.FormattedAddress(),
			Denom:  testvalues.DemoDenom,
		}
		createBridgeMsg := &wfchain.MsgCreateBridge{
			From:                  simdUser.FormattedAddress(),
			Denom:                 testvalues.DemoDenom,
			ClientId:              testvalues.FirstWasmClientID,
			RemoteContractAddress: ibcxerc20Address.String(),
		}
		_, err := s.BroadcastMessages(ctx, simd, simdUser, 500_000, createDenomMsg, createBridgeMsg)
		s.Require().NoError(err)
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

func (s *DemoTestSuite) Test_BridgeTransferFromEth() {
	ctx := context.Background()
	proofType := types.GetEnvProofType()
	s.BridgeTransferFromEthTest(ctx, proofType)
}

func (s *DemoTestSuite) BridgeTransferFromEthTest(ctx context.Context, proofType types.SupportedProofType) {
	s.SetupSuite(ctx, proofType)

	eth, simd := s.EthChain, s.CosmosChains[0]
	simdUser := s.CosmosUsers[0]
	ethUserAddress := crypto.PubkeyToAddress(s.key.PublicKey)

	ics26Address := ethcommon.HexToAddress(s.contractAddresses.Ics26Router)

	amount := big.NewInt(1_000)
	s.Require().True(s.Run("Fund user with IBCXERC20", func() {
		_, err := s.ibcXERC20.Transfer(s.GetTransactOpts(s.EthChain.Faucet, eth), ethUserAddress, amount)
		s.Require().NoError(err)
	}))

	var sendTxHash []byte
	s.Require().True(s.Run("Send tokens on Ethereum", func() {
		tx, err := s.ibcXERC20.BridgeTransfer(s.GetTransactOpts(s.key, eth), simdUser.FormattedAddress(), amount)
		s.Require().NoError(err)

		receipt, err := eth.GetTxReciept(ctx, tx.Hash())
		s.Require().NoError(err)
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)
		sendTxHash = tx.Hash().Bytes()

		s.Require().True(s.Run("Verify balances on Ethereum", func() {
			// User balance on Ethereum
			bal, err := s.ibcXERC20.BalanceOf(nil, ethUserAddress)
			s.Require().NoError(err)
			s.Require().Zero(bal.Int64())
		}))
	}))

	var ackTxHash []byte
	s.Require().True(s.Run("Relay Packet", func() {
		var relayTxBodyBz []byte
		s.Require().True(s.Run("Retrieve relay tx", func() {
			resp, err := s.RelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
				SrcChain:    eth.ChainID.String(),
				DstChain:    simd.Config().ChainID,
				SourceTxIds: [][]byte{sendTxHash},
				SrcClientId: testvalues.CustomClientID,
				DstClientId: testvalues.FirstWasmClientID,
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Tx)
			s.Require().Empty(resp.Address)

			relayTxBodyBz = resp.Tx
		}))

		s.Require().True(s.Run("Broadcast relay tx", func() {
			resp := s.MustBroadcastSdkTxBody(ctx, simd, s.SimdRelayerSubmitter, 2_000_000, relayTxBodyBz)

			var err error
			ackTxHash, err = hex.DecodeString(resp.TxHash)
			s.Require().NoError(err)
			s.Require().NotEmpty(ackTxHash)
		}))

		s.Require().True(s.Run("Verify balances on Cosmos chain", func() {
			// User balance on Cosmos chain
			resp, err := e2esuite.GRPCQuery[banktypes.QueryBalanceResponse](ctx, simd, &banktypes.QueryBalanceRequest{
				Address: simdUser.FormattedAddress(),
				Denom:   testvalues.DemoDenom,
			})
			s.Require().NoError(err)
			s.Require().NotNil(resp.Balance)
			s.Require().Equal(amount, resp.Balance.Amount.BigInt())
			s.Require().Equal(testvalues.DemoDenom, resp.Balance.Denom)
		}))
	}))

	s.Require().True(s.Run("Relay Acknowledgement", func() {
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
			receipt, err := eth.BroadcastTx(ctx, s.EthRelayerSubmitter, 1_000_000, &ics26Address, ackRelayTx)
			s.Require().NoError(err)
			s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status, fmt.Sprintf("Tx failed: %+v", receipt))

			// Verify the ack packet event exists
			_, err = e2esuite.GetEvmEvent(receipt, s.ics26Contract.ParseAckPacket)
			s.Require().NoError(err)
		}))

		s.Require().True(s.Run("Verify balances on Ethereum", func() {
			// User balance on Ethereum
			bal, err := s.ibcXERC20.BalanceOf(nil, ethUserAddress)
			s.Require().NoError(err)
			s.Require().Zero(bal.Int64())
		}))
	}))
}

func (s *DemoTestSuite) Test_BridgeTransferFromCosmos() {
	ctx := context.Background()
	proofType := types.GetEnvProofType()
	s.BridgeTransferFromCosmosTest(ctx, proofType)
}

func (s *DemoTestSuite) BridgeTransferFromCosmosTest(ctx context.Context, proofType types.SupportedProofType) {
	s.SetupSuite(ctx, proofType)

	eth, simd := s.EthChain, s.CosmosChains[0]
	simdUser := s.CosmosUsers[0]
	ethUserAddress := crypto.PubkeyToAddress(s.key.PublicKey)

	ics26Address := ethcommon.HexToAddress(s.contractAddresses.Ics26Router)

	amount := sdkmath.NewInt(1_000)
	s.Require().True(s.Run("Fund user with denom", func() {
		mintMsg := &wfchain.MsgMint{
			From:    simdUser.FormattedAddress(),
			Address: simdUser.FormattedAddress(),
			Amount:  sdk.NewCoin(testvalues.DemoDenom, amount),
		}
		_, err := s.BroadcastMessages(ctx, simd, simdUser, 500_000, mintMsg)
		s.Require().NoError(err)
	}))

	var sendTxHash []byte
	s.Require().True(s.Run("Send tokens on Cosmos chain", func() {
		transferMsg := &wfchain.MsgBridgeTransfer{
			Sender:           simdUser.FormattedAddress(),
			Denom:            testvalues.DemoDenom,
			Amount:           amount.BigInt().String(),
			Receiver:         ethUserAddress.String(),
			SourceClient:     testvalues.FirstWasmClientID,
			TimeoutTimestamp: uint64(time.Now().Add(1 * time.Hour).Unix()),
		}
		resp, err := s.BroadcastMessages(ctx, simd, simdUser, 500_000, transferMsg)
		s.Require().NoError(err)
		s.Require().NotEmpty(resp.TxHash)

		sendTxHash, err = hex.DecodeString(resp.TxHash)
		s.Require().NoError(err)
		s.Require().NotEmpty(sendTxHash)

		s.Require().True(s.Run("Verify balances on Cosmos chain", func() {
			// User balance on Cosmos chain
			resp, err := e2esuite.GRPCQuery[banktypes.QueryBalanceResponse](ctx, simd, &banktypes.QueryBalanceRequest{
				Address: simdUser.FormattedAddress(),
				Denom:   testvalues.DemoDenom,
			})
			s.Require().NoError(err)
			s.Require().NotNil(resp.Balance)
			s.Require().Zero(resp.Balance.Amount.Int64())
			s.Require().Equal(testvalues.DemoDenom, resp.Balance.Denom)
		}))
	}))

	var ackTxHash []byte
	s.Require().True(s.Run("Relay Packet", func() {
		var relayTxBodyBz []byte
		s.Require().True(s.Run("Retrieve relay tx", func() {
			resp, err := s.RelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
				SrcChain:    simd.Config().ChainID,
				DstChain:    eth.ChainID.String(),
				SourceTxIds: [][]byte{sendTxHash},
				SrcClientId: testvalues.FirstWasmClientID,
				DstClientId: testvalues.CustomClientID,
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Tx)
			s.Require().Equal(resp.Address, ics26Address.String())

			relayTxBodyBz = resp.Tx
		}))

		s.Require().True(s.Run("Submit relay tx", func() {
			receipt, err := eth.BroadcastTx(ctx, s.EthRelayerSubmitter, 1_000_000, &ics26Address, relayTxBodyBz)
			s.Require().NoError(err)
			s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status, fmt.Sprintf("Tx failed: %+v", receipt))

			_, err = e2esuite.GetEvmEvent(receipt, s.ics26Contract.ParseAckPacket)
			s.Require().NoError(err)

			ackTxHash = receipt.TxHash.Bytes()
			s.Require().NotEmpty(ackTxHash)
		}))

		s.Require().True(s.Run("Verify balances on Ethereum", func() {
			// User balance on Ethereum
			bal, err := s.ibcXERC20.BalanceOf(nil, ethUserAddress)
			s.Require().NoError(err)
			s.Require().Equal(amount.Int64(), bal.Int64())
		}))
	}))
}
