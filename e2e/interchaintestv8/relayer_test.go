package main

import (
	"context"
	"encoding/hex"
	"math/big"
	"os"
	"strings"
	"testing"
	"time"

	"github.com/cosmos/solidity-ibc-eureka/abigen/ibcerc20"
	"github.com/cosmos/solidity-ibc-eureka/abigen/ics20lib"
	"github.com/stretchr/testify/suite"

	ethcommon "github.com/ethereum/go-ethereum/common"
	ethtypes "github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/crypto"
	"github.com/ethereum/go-ethereum/ethclient"

	sdkmath "cosmossdk.io/math"

	sdk "github.com/cosmos/cosmos-sdk/types"
	banktypes "github.com/cosmos/cosmos-sdk/x/bank/types"

	transfertypes "github.com/cosmos/ibc-go/v9/modules/apps/transfer/types"
	channeltypesv2 "github.com/cosmos/ibc-go/v9/modules/core/04-channel/v2/types"
	ibctesting "github.com/cosmos/ibc-go/v9/testing"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/e2esuite"
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
		configInfo := relayer.ConfigInfo{
			TmRPC:         simd.GetHostRPCAddress(),
			ICS26Address:  s.contractAddresses.Ics26Router,
			EthRPC:        eth.RPC,
			SP1PrivateKey: os.Getenv(testvalues.EnvKeySp1PrivateKey),
		}

		err := configInfo.GenerateConfigFile(testvalues.RelayerConfigFilePath)
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

func (s *RelayerTestSuite) TestRelayToEth() {
	ctx := context.Background()
	s.SetupSuite(ctx, operator.ProofTypeGroth16)

	eth, simd := s.ChainA, s.ChainB

	ics26Address := ethcommon.HexToAddress(s.contractAddresses.Ics26Router)
	ics20Address := ethcommon.HexToAddress(s.contractAddresses.Ics20Transfer)
	transferAmount := big.NewInt(testvalues.TransferAmount)
	ethereumUserAddress := crypto.PubkeyToAddress(s.key.PublicKey)
	cosmosUserWallet := s.UserB
	cosmosUserAddress := cosmosUserWallet.FormattedAddress()
	sendMemo := "nonnativesend"

	var (
		transferCoin sdk.Coin
		txHash       []byte
	)
	s.Require().True(s.Run("Send transfer on Cosmos chain", func() {
		timeout := uint64(time.Now().Add(30 * time.Minute).Unix())
		transferCoin = sdk.NewCoin(s.ChainB.Config().Denom, sdkmath.NewIntFromBigInt(transferAmount))

		transferPayload := ics20lib.ICS20LibFungibleTokenPacketData{
			Denom:    transferCoin.Denom,
			Amount:   transferCoin.Amount.BigInt(),
			Sender:   cosmosUserAddress,
			Receiver: strings.ToLower(ethereumUserAddress.Hex()),
			Memo:     sendMemo,
		}
		transferBz, err := ics20lib.EncodeFungibleTokenPacketData(transferPayload)
		s.Require().NoError(err)

		payload := channeltypesv2.Payload{
			SourcePort:      transfertypes.PortID,
			DestinationPort: transfertypes.PortID,
			Version:         transfertypes.V1,
			Encoding:        transfertypes.EncodingABI,
			Value:           transferBz,
		}
		msgSendPacket := channeltypesv2.MsgSendPacket{
			SourceChannel:    ibctesting.FirstChannelID,
			TimeoutTimestamp: timeout,
			Payloads: []channeltypesv2.Payload{
				payload,
			},
			Signer: cosmosUserWallet.FormattedAddress(),
		}

		resp, err := s.BroadcastMessages(ctx, simd, cosmosUserWallet, 200_000, &msgSendPacket)
		s.Require().NoError(err)
		s.Require().NotEmpty(resp.TxHash)

		txHash, err = hex.DecodeString(resp.TxHash)
		s.Require().NoError(err)

		s.Require().True(s.Run("Verify balances on Cosmos chain", func() {
			// Check the balance of UserB
			resp, err := e2esuite.GRPCQuery[banktypes.QueryBalanceResponse](ctx, simd, &banktypes.QueryBalanceRequest{
				Address: cosmosUserAddress,
				Denom:   transferCoin.Denom,
			})
			s.Require().NoError(err)
			s.Require().NotNil(resp.Balance)
			s.Require().Equal(testvalues.InitialBalance-testvalues.TransferAmount, resp.Balance.Amount.Int64())
		}))
	}))

	var multicallTx []byte
	s.Require().True(s.Run("Retrieve relay tx to Ethereum", func() {
		resp, err := s.RelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
			SourceTxIds:     [][]byte{txHash},
			TargetChannelId: s.TendermintLightClientID,
		})
		s.Require().NoError(err)
		s.Require().NotEmpty(resp.Tx)
		s.Require().Equal(resp.Address, ics26Address.String())

		multicallTx = resp.Tx
	}))

	s.Require().True(s.Run("Submit relay tx to Ethereum", func() {
		ethClient, err := ethclient.Dial(eth.RPC)
		s.Require().NoError(err)

		txOpts := s.GetTransactOpts(s.key, eth)
		s.Require().NoError(err)

		tx := ethtypes.NewTransaction(
			txOpts.Nonce.Uint64(),
			ics26Address,
			txOpts.Value,
			5_000_000,
			txOpts.GasPrice,
			multicallTx,
		)

		signedTx, err := txOpts.Signer(txOpts.From, tx)
		s.Require().NoError(err)

		// Submit the relay tx to Ethereum
		err = ethClient.SendTransaction(ctx, signedTx)
		s.Require().NoError(err)

		// Wait for the tx to be mined
		receipt := s.GetTxReciept(ctx, eth, signedTx.Hash())
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)

		s.True(s.Run("Verify balances on Ethereum", func() {
			ethReceiveTransferEvent, err := e2esuite.GetEvmEvent(receipt, s.ics20Contract.ParseICS20ReceiveTransfer)
			s.Require().NoError(err)

			ethClient, err := ethclient.Dial(eth.RPC)
			s.Require().NoError(err)
			ibcERC20, err := ibcerc20.NewContract(ethReceiveTransferEvent.Erc20Address, ethClient)
			s.Require().NoError(err)

			// User balance on Ethereum
			userBalance, err := ibcERC20.BalanceOf(nil, ethereumUserAddress)
			s.Require().NoError(err)
			s.Require().Equal(transferAmount, userBalance)

			// ICS20 contract balance on Ethereum
			ics20TransferBalance, err := ibcERC20.BalanceOf(nil, ics20Address)
			s.Require().NoError(err)
			s.Require().Equal(int64(0), ics20TransferBalance.Int64())
		}))
	}))
}
