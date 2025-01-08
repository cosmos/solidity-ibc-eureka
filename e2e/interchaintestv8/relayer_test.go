package main

import (
	"context"
	"encoding/hex"
	"fmt"
	"math/big"
	"os"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/cosmos/gogoproto/proto"
	"github.com/cosmos/solidity-ibc-eureka/abigen/ics20lib"
	"github.com/cosmos/solidity-ibc-eureka/abigen/ics26router"
	"github.com/stretchr/testify/suite"

	"github.com/ethereum/go-ethereum/accounts/abi"
	ethcommon "github.com/ethereum/go-ethereum/common"
	ethtypes "github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/crypto"
	"github.com/ethereum/go-ethereum/ethclient"

	sdkmath "cosmossdk.io/math"

	sdk "github.com/cosmos/cosmos-sdk/types"
	txtypes "github.com/cosmos/cosmos-sdk/types/tx"
	banktypes "github.com/cosmos/cosmos-sdk/x/bank/types"

	transfertypes "github.com/cosmos/ibc-go/v9/modules/apps/transfer/types"
	channeltypesv2 "github.com/cosmos/ibc-go/v9/modules/core/04-channel/v2/types"
	ibctesting "github.com/cosmos/ibc-go/v9/testing"

	"github.com/strangelove-ventures/interchaintest/v8/ibc"

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

	SimdSubmitter ibc.Wallet

	EthToCosmosRelayerClient relayertypes.RelayerServiceClient
	CosmosToEthRelayerClient relayertypes.RelayerServiceClient
}

// TestWithIbcEurekaTestSuite is the boilerplate code that allows the test suite to be run
func TestWithRelayerTestSuite(t *testing.T) {
	suite.Run(t, new(RelayerTestSuite))
}

// SetupSuite is called once, before the start of the test suite
func (s *RelayerTestSuite) SetupSuite(ctx context.Context, proofType operator.SupportedProofType) {
	s.IbcEurekaTestSuite.SetupSuite(ctx, proofType)

	eth, simd := s.EthChain, s.CosmosChains[0]

	s.SimdSubmitter = s.CreateAndFundCosmosUser(ctx, simd)

	var relayerProcess *os.Process
	var configInfo relayer.EthCosmosConfigInfo
	s.Require().True(s.Run("Start Relayer", func() {
		beaconAPI := ""
		// The BeaconAPIClient is nil when the testnet is `pow`
		if eth.BeaconAPIClient != nil {
			beaconAPI = eth.BeaconAPIClient.GetBeaconAPIURL()
		}

		configInfo = relayer.EthCosmosConfigInfo{
			EthToCosmosPort: 3000,
			CosmosToEthPort: 3001,
			TmRPC:           simd.GetHostRPCAddress(),
			ICS26Address:    s.contractAddresses.Ics26Router,
			EthRPC:          eth.RPC,
			BeaconAPI:       beaconAPI,
			SP1PrivateKey:   os.Getenv(testvalues.EnvKeySp1PrivateKey),
			SignerAddress:   s.SimdSubmitter.FormattedAddress(),
			Mock:            os.Getenv(testvalues.EnvKeyEthTestnetType) == testvalues.EthTestnetTypePoW,
		}

		err := configInfo.GenerateEthCosmosConfigFile(testvalues.RelayerConfigFilePath)
		s.Require().NoError(err)

		relayerProcess, err = relayer.StartRelayer(testvalues.RelayerConfigFilePath)
		s.Require().NoError(err)

		s.T().Cleanup(func() {
			os.Remove(testvalues.RelayerConfigFilePath)
		})
	}))

	s.T().Cleanup(func() {
		if relayerProcess != nil {
			err := relayerProcess.Kill()
			if err != nil {
				s.T().Logf("Failed to kill the relayer process: %v", err)
			}
		}
	})

	s.Require().True(s.Run("Create Relayer Clients", func() {
		var err error
		s.EthToCosmosRelayerClient, err = relayer.GetGRPCClient(configInfo.EthToCosmosGRPCAddress())
		s.Require().NoError(err)

		s.CosmosToEthRelayerClient, err = relayer.GetGRPCClient(configInfo.CosmosToEthGRPCAddress())
		s.Require().NoError(err)
	}))
}

// TestRelayer is a test that runs the relayer
func (s *RelayerTestSuite) TestRelayerInfo() {
	ctx := context.Background()
	s.SetupSuite(ctx, operator.ProofTypeGroth16)

	eth, simd := s.EthChain, s.CosmosChains[0]

	s.Run("Cosmos to Eth Relayer Info", func() {
		info, err := s.CosmosToEthRelayerClient.Info(context.Background(), &relayertypes.InfoRequest{})
		s.Require().NoError(err)
		s.Require().NotNil(info)

		s.T().Logf("Relayer Info: %+v", info)

		s.Require().Equal(simd.Config().ChainID, info.SourceChain.ChainId)
		s.Require().Equal(eth.ChainID.String(), info.TargetChain.ChainId)
	})

	s.Run("Eth to Cosmos Relayer Info", func() {
		info, err := s.EthToCosmosRelayerClient.Info(context.Background(), &relayertypes.InfoRequest{})
		s.Require().NoError(err)
		s.Require().NotNil(info)

		s.T().Logf("Relayer Info: %+v", info)

		s.Require().Equal(eth.ChainID.String(), info.SourceChain.ChainId)
		s.Require().Equal(simd.Config().ChainID, info.TargetChain.ChainId)
	})
}

func (s *RelayerTestSuite) TestRecvPacketToEth_Groth16() {
	ctx := context.Background()
	s.RecvPacketToEthTest(ctx, operator.ProofTypeGroth16, 1)
}

func (s *RelayerTestSuite) TestRecvPacketToEth_Plonk() {
	ctx := context.Background()
	s.RecvPacketToEthTest(ctx, operator.ProofTypePlonk, 1)
}

func (s *RelayerTestSuite) Test_10_RecvPacketToEth_Groth16() {
	ctx := context.Background()
	s.RecvPacketToEthTest(ctx, operator.ProofTypeGroth16, 10)
}

func (s *RelayerTestSuite) Test_5_RecvPacketToEth_Plonk() {
	ctx := context.Background()
	s.RecvPacketToEthTest(ctx, operator.ProofTypePlonk, 5)
}

func (s *RelayerTestSuite) RecvPacketToEthTest(
	ctx context.Context, proofType operator.SupportedProofType, numOfTransfers int,
) {
	s.Require().Greater(numOfTransfers, 0)

	s.SetupSuite(ctx, proofType)

	eth, simd := s.EthChain, s.CosmosChains[0]

	ics26Address := ethcommon.HexToAddress(s.contractAddresses.Ics26Router)
	transferAmount := big.NewInt(testvalues.TransferAmount)
	totalTransferAmount := big.NewInt(testvalues.TransferAmount * int64(numOfTransfers))
	if totalTransferAmount.Int64() > testvalues.InitialBalance {
		s.FailNow("Total transfer amount exceeds the initial balance")
	}
	ethereumUserAddress := crypto.PubkeyToAddress(s.key.PublicKey)
	cosmosUserWallet := s.CosmosUsers[0]
	cosmosUserAddress := cosmosUserWallet.FormattedAddress()
	sendMemo := "nonnativesend"

	var (
		transferCoin sdk.Coin
		txHashes     [][]byte
	)
	s.Require().True(s.Run("Send transfers on Cosmos chain", func() {
		for i := 0; i < numOfTransfers; i++ {
			timeout := uint64(time.Now().Add(30 * time.Minute).Unix())
			transferCoin = sdk.NewCoin(simd.Config().Denom, sdkmath.NewIntFromBigInt(transferAmount))

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

			txHash, err := hex.DecodeString(resp.TxHash)
			s.Require().NoError(err)

			txHashes = append(txHashes, txHash)
		}

		s.Require().True(s.Run("Verify balances on Cosmos chain", func() {
			// Check the balance of UserB
			resp, err := e2esuite.GRPCQuery[banktypes.QueryBalanceResponse](ctx, simd, &banktypes.QueryBalanceRequest{
				Address: cosmosUserAddress,
				Denom:   transferCoin.Denom,
			})
			s.Require().NoError(err)
			s.Require().NotNil(resp.Balance)
			s.Require().Equal(testvalues.InitialBalance-totalTransferAmount.Int64(), resp.Balance.Amount.Int64())
		}))
	}))

	var multicallTx []byte
	s.Require().True(s.Run("Retrieve relay tx to Ethereum", func() {
		resp, err := s.CosmosToEthRelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
			SourceTxIds:     txHashes,
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
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status, fmt.Sprintf("Tx failed: %+v", receipt))

		/* Commenting out this part for now, once the test with removed event work we can update it
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
			s.Require().Equal(totalTransferAmount, userBalance)

			// ICS20 contract balance on Ethereum
			ics20TransferBalance, err := ibcERC20.BalanceOf(nil, ics20Address)
			s.Require().NoError(err)
			s.Require().Equal(int64(0), ics20TransferBalance.Int64())
		}))
		*/
	}))
}

func (s *RelayerTestSuite) Test_2_ConcurrentRecvPacketToEth_Groth16() {
	// I've noticed that the prover network drops the requests when sending too many
	ctx := context.Background()
	s.ConcurrentRecvPacketToEthTest(ctx, operator.ProofTypeGroth16, 2)
}

func (s *RelayerTestSuite) ConcurrentRecvPacketToEthTest(
	ctx context.Context, proofType operator.SupportedProofType, numConcurrentTransfers int,
) {
	s.Require().Greater(numConcurrentTransfers, 0)

	s.SetupSuite(ctx, proofType)

	_, simd := s.EthChain, s.CosmosChains[0]

	ics26Address := ethcommon.HexToAddress(s.contractAddresses.Ics26Router)
	transferAmount := big.NewInt(testvalues.TransferAmount)
	totalTransferAmount := big.NewInt(testvalues.TransferAmount * int64(numConcurrentTransfers))
	if totalTransferAmount.Int64() > testvalues.InitialBalance {
		s.FailNow("Total transfer amount exceeds the initial balance")
	}
	ethereumUserAddress := crypto.PubkeyToAddress(s.key.PublicKey)
	cosmosUserWallet := s.CosmosUsers[0]
	cosmosUserAddress := cosmosUserWallet.FormattedAddress()

	var (
		transferCoin sdk.Coin
		txHashes     [][]byte
	)
	s.Require().True(s.Run("Send transfers on Cosmos chain", func() {
		for i := 0; i < numConcurrentTransfers; i++ {
			timeout := uint64(time.Now().Add(30 * time.Minute).Unix())
			transferCoin = sdk.NewCoin(simd.Config().Denom, sdkmath.NewIntFromBigInt(transferAmount))

			transferPayload := ics20lib.ICS20LibFungibleTokenPacketData{
				Denom:    transferCoin.Denom,
				Amount:   transferCoin.Amount.BigInt(),
				Sender:   cosmosUserAddress,
				Receiver: strings.ToLower(ethereumUserAddress.Hex()),
				Memo:     "",
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

			txHash, err := hex.DecodeString(resp.TxHash)
			s.Require().NoError(err)

			txHashes = append(txHashes, txHash)
		}

		s.Require().True(s.Run("Verify balances on Cosmos chain", func() {
			// Check the balance of UserB
			resp, err := e2esuite.GRPCQuery[banktypes.QueryBalanceResponse](ctx, simd, &banktypes.QueryBalanceRequest{
				Address: cosmosUserAddress,
				Denom:   transferCoin.Denom,
			})
			s.Require().NoError(err)
			s.Require().NotNil(resp.Balance)
			s.Require().Equal(testvalues.InitialBalance-totalTransferAmount.Int64(), resp.Balance.Amount.Int64())
		}))
	}))

	s.Require().True(s.Run("Install circuit artifacts on machine", func() {
		// When running multiple instances of the relayer, the circuit artifacts need to be installed on the machine
		// to avoid the overhead of installing the artifacts for each relayer instance (which also panics).
		// This is why we make a single request which installs the artifacts on the machine, and discard the response.

		resp, err := s.CosmosToEthRelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
			SourceTxIds:     txHashes,
			TargetChannelId: s.TendermintLightClientID,
		})
		s.Require().NoError(err)
		s.Require().NotEmpty(resp.Tx)
		s.Require().Equal(resp.Address, ics26Address.String())
	}))

	var wg sync.WaitGroup
	wg.Add(numConcurrentTransfers)
	s.Require().True(s.Run("Make concurrent requests", func() {
		// loop over the txHashes and send them concurrently
		for _, txHash := range txHashes {
			// we send the request while the previous request is still being processed
			time.Sleep(3 * time.Second)
			go func() {
				defer wg.Done() // decrement the counter when the request completes
				resp, err := s.CosmosToEthRelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
					SourceTxIds:     [][]byte{txHash},
					TargetChannelId: s.TendermintLightClientID,
				})
				s.Require().NoError(err)
				s.Require().NotEmpty(resp.Tx)
				s.Require().Equal(resp.Address, ics26Address.String())
			}()
		}
	}))

	s.Require().True(s.Run("Wait for all requests to complete", func() {
		// wait for all requests to complete
		// If the request never completes, we rely on the test timeout to kill the test
		wg.Wait()
	}))
}

func (s *RelayerTestSuite) TestBatchedAckPacketToEth_Groth16() {
	ctx := context.Background()
	s.ICS20TransferERC20TokenBatchedAckTest(ctx, operator.ProofTypeGroth16, 1)
}

func (s *RelayerTestSuite) TestBatchedAckPacketToEth_Plonk() {
	ctx := context.Background()
	s.ICS20TransferERC20TokenBatchedAckTest(ctx, operator.ProofTypePlonk, 1)
}

func (s *RelayerTestSuite) Test_10_BatchedAckPacketToEth_Groth16() {
	ctx := context.Background()
	s.ICS20TransferERC20TokenBatchedAckTest(ctx, operator.ProofTypeGroth16, 10)
}

func (s *RelayerTestSuite) Test_5_BatchedAckPacketToEth_Plonk() {
	ctx := context.Background()
	s.ICS20TransferERC20TokenBatchedAckTest(ctx, operator.ProofTypePlonk, 5)
}

// Note that the relayer still only relays one tx, the batching is done
// on the cosmos transaction itself. So that it emits multiple IBC events.
func (s *RelayerTestSuite) ICS20TransferERC20TokenBatchedAckTest(
	ctx context.Context, proofType operator.SupportedProofType, numOfTransfers int,
) {
	s.SetupSuite(ctx, proofType)

	eth, simd := s.EthChain, s.CosmosChains[0]

	ics26Address := ethcommon.HexToAddress(s.contractAddresses.Ics26Router)
	ics20Address := ethcommon.HexToAddress(s.contractAddresses.Ics20Transfer)
	transferAmount := big.NewInt(testvalues.TransferAmount)
	totalTransferAmount := big.NewInt(testvalues.TransferAmount * int64(numOfTransfers)) // total amount transferred
	if totalTransferAmount.Int64() > testvalues.InitialBalance {
		s.FailNow("Total transfer amount exceeds the initial balance")
	}
	ethereumUserAddress := crypto.PubkeyToAddress(s.key.PublicKey)
	cosmosUserWallet := s.CosmosUsers[0]
	cosmosUserAddress := cosmosUserWallet.FormattedAddress()

	ics26routerAbi, err := abi.JSON(strings.NewReader(ics26router.ContractABI))
	s.Require().NoError(err)

	s.Require().True(s.Run("Approve the ICS20Transfer.sol contract to spend the erc20 tokens", func() {
		tx, err := s.erc20Contract.Approve(s.GetTransactOpts(s.key, eth), ics20Address, totalTransferAmount)
		s.Require().NoError(err)
		receipt := s.GetTxReciept(ctx, eth, tx.Hash())
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)

		allowance, err := s.erc20Contract.Allowance(nil, ethereumUserAddress, ics20Address)
		s.Require().NoError(err)
		s.Require().Equal(totalTransferAmount, allowance)
	}))

	var txHashes [][]byte
	s.Require().True(s.Run(fmt.Sprintf("Send %d transfers on Ethereum", numOfTransfers), func() {
		timeout := uint64(time.Now().Add(30 * time.Minute).Unix())
		transferMulticall := make([][]byte, numOfTransfers)

		msgSendPacket := s.createICS20MsgSendPacket(
			ethereumUserAddress,
			s.contractAddresses.Erc20,
			transferAmount,
			cosmosUserAddress,
			s.TendermintLightClientID,
			timeout,
			"",
		)

		encodedMsg, err := ics26routerAbi.Pack("sendPacket", msgSendPacket)
		s.Require().NoError(err)
		for i := 0; i < numOfTransfers; i++ {
			transferMulticall[i] = encodedMsg
		}

		tx, err := s.ics26Contract.Multicall(s.GetTransactOpts(s.key, eth), transferMulticall)
		s.Require().NoError(err)
		receipt := s.GetTxReciept(ctx, eth, tx.Hash())
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)
		s.T().Logf("Multicall send %d transfers gas used: %d", numOfTransfers, receipt.GasUsed)
		txHashes = append(txHashes, tx.Hash().Bytes())

		s.True(s.Run("Verify balances on Ethereum", func() {
			// User balance on Ethereum
			userBalance, err := s.erc20Contract.BalanceOf(nil, ethereumUserAddress)
			s.Require().NoError(err)
			s.Require().Equal(testvalues.InitialBalance-totalTransferAmount.Int64(), userBalance.Int64())

			// ICS20 contract balance on Ethereum
			escrowBalance, err := s.erc20Contract.BalanceOf(nil, s.escrowContractAddr)
			s.Require().NoError(err)
			s.Require().Equal(totalTransferAmount, escrowBalance)
		}))
	}))

	var txHash []byte
	s.Require().True(s.Run("Receive packets on Cosmos chain", func() {
		var txBodyBz []byte
		s.Require().True(s.Run("Retrieve relay tx to Cosmos chain", func() {
			resp, err := s.EthToCosmosRelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
				SourceTxIds:     txHashes,
				TargetChannelId: ibctesting.FirstChannelID,
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Tx)
			s.Require().Empty(resp.Address)

			txBodyBz = resp.Tx
		}))

		s.Require().True(s.Run("Broadcast relay tx on Cosmos chain", func() {
			var txBody txtypes.TxBody
			err := proto.Unmarshal(txBodyBz, &txBody)
			s.Require().NoError(err)

			var msgs []sdk.Msg
			for _, msg := range txBody.Messages {
				var sdkMsg sdk.Msg
				err = simd.Config().EncodingConfig.InterfaceRegistry.UnpackAny(msg, &sdkMsg)
				s.Require().NoError(err)

				msgs = append(msgs, sdkMsg)
			}

			s.Require().NotZero(len(msgs))

			resp, err := s.BroadcastMessages(ctx, simd, s.SimdSubmitter, 2_000_000, msgs...)
			s.Require().NoError(err)

			txHash, err = hex.DecodeString(resp.TxHash)
			s.Require().NoError(err)
			s.Require().NotEmpty(txHash)
		}))

		s.Require().True(s.Run("Verify balances on Cosmos chain", func() {
			denomOnCosmos := transfertypes.NewDenom(s.contractAddresses.Erc20, transfertypes.NewHop(transfertypes.PortID, ibctesting.FirstChannelID))

			// User balance on Cosmos chain
			resp, err := e2esuite.GRPCQuery[banktypes.QueryBalanceResponse](ctx, simd, &banktypes.QueryBalanceRequest{
				Address: cosmosUserAddress,
				Denom:   denomOnCosmos.IBCDenom(),
			})
			s.Require().NoError(err)
			s.Require().NotNil(resp.Balance)
			s.Require().Equal(totalTransferAmount.Uint64(), resp.Balance.Amount.Uint64())
			s.Require().Equal(denomOnCosmos.IBCDenom(), resp.Balance.Denom)
		}))
	}))

	s.Require().True(s.Run("Acknowledge packets on Ethereum", func() {
		var multicallTx []byte
		s.Require().True(s.Run("Retrieve relay tx to Ethereum", func() {
			resp, err := s.CosmosToEthRelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
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

			// Verify the ack packet event exists
			_, err = e2esuite.GetEvmEvent(receipt, s.ics26Contract.ParseAckPacket)
			s.Require().NoError(err)
		}))

		s.Require().True(s.Run("Verify balances on Ethereum", func() {
			// User balance on Ethereum
			userBalance, err := s.erc20Contract.BalanceOf(nil, ethereumUserAddress)
			s.Require().NoError(err)
			s.Require().Equal(testvalues.InitialBalance-totalTransferAmount.Int64(), userBalance.Int64())

			// ICS20 contract balance on Ethereum
			escrowBalance, err := s.erc20Contract.BalanceOf(nil, s.escrowContractAddr)
			s.Require().NoError(err)
			s.Require().Equal(totalTransferAmount.Int64(), escrowBalance.Int64())
		}))
	}))
}

func (s *RelayerTestSuite) TestTimeoutPacketFromEth_Groth16() {
	ctx := context.Background()
	s.ICS20TimeoutFromEthereumToTimeoutTest(ctx, operator.ProofTypeGroth16, 1)
}

func (s *RelayerTestSuite) TestTimeoutPacketFromEth_Plonk() {
	ctx := context.Background()
	s.ICS20TimeoutFromEthereumToTimeoutTest(ctx, operator.ProofTypePlonk, 1)
}

func (s *RelayerTestSuite) Test_10_TimeoutPacketFromEth_Groth16() {
	ctx := context.Background()
	s.ICS20TimeoutFromEthereumToTimeoutTest(ctx, operator.ProofTypeGroth16, 10)
}

func (s *RelayerTestSuite) Test_5_TimeoutPacketFromEth_Plonk() {
	ctx := context.Background()
	s.ICS20TimeoutFromEthereumToTimeoutTest(ctx, operator.ProofTypePlonk, 5)
}

func (s *RelayerTestSuite) ICS20TimeoutFromEthereumToTimeoutTest(
	ctx context.Context, pt operator.SupportedProofType, numOfTransfers int,
) {
	s.SetupSuite(ctx, pt)

	eth, _ := s.EthChain, s.CosmosChains[0]

	ics26Address := ethcommon.HexToAddress(s.contractAddresses.Ics26Router)
	transferAmount := big.NewInt(testvalues.TransferAmount)
	totalTransferAmount := big.NewInt(testvalues.TransferAmount * int64(numOfTransfers)) // total amount transferred
	if totalTransferAmount.Int64() > testvalues.InitialBalance {
		s.FailNow("Total transfer amount exceeds the initial balance")
	}
	ethereumUserAddress := crypto.PubkeyToAddress(s.key.PublicKey)
	cosmosUserWallet := s.CosmosUsers[0]
	cosmosUserAddress := cosmosUserWallet.FormattedAddress()

	s.Require().True(s.Run("Approve the ICS20Transfer.sol contract to spend the erc20 tokens", func() {
		ics20Address := ethcommon.HexToAddress(s.contractAddresses.Ics20Transfer)
		tx, err := s.erc20Contract.Approve(s.GetTransactOpts(s.key, eth), ics20Address, totalTransferAmount)
		s.Require().NoError(err)
		receipt := s.GetTxReciept(ctx, eth, tx.Hash())
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)

		allowance, err := s.erc20Contract.Allowance(nil, ethereumUserAddress, ics20Address)
		s.Require().NoError(err)
		s.Require().Equal(totalTransferAmount, allowance)
	}))

	var txHashes [][]byte
	s.Require().True(s.Run("Send transfer on Ethereum", func() {
		for i := 0; i < numOfTransfers; i++ {
			timeout := uint64(time.Now().Add(30 * time.Second).Unix())

			msgSendPacket := s.createICS20MsgSendPacket(
				ethereumUserAddress,
				s.contractAddresses.Erc20,
				transferAmount,
				cosmosUserAddress,
				s.TendermintLightClientID,
				timeout,
				"testmemo",
			)

			tx, err := s.ics26Contract.SendPacket(s.GetTransactOpts(s.key, eth), msgSendPacket)
			s.Require().NoError(err)
			receipt := s.GetTxReciept(ctx, eth, tx.Hash())
			s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)

			txHashes = append(txHashes, tx.Hash().Bytes())
		}

		s.Require().True(s.Run("Verify balances on Ethereum", func() {
			// User balance on Ethereum
			userBalance, err := s.erc20Contract.BalanceOf(nil, ethereumUserAddress)
			s.Require().NoError(err)
			s.Require().Equal(testvalues.InitialBalance-totalTransferAmount.Int64(), userBalance.Int64())

			// ICS20 contract balance on Ethereum
			escrowBalance, err := s.erc20Contract.BalanceOf(nil, s.escrowContractAddr)
			s.Require().NoError(err)
			s.Require().Equal(totalTransferAmount, escrowBalance)
		}))
	}))

	// sleep for 45 seconds to let the packet timeout
	time.Sleep(45 * time.Second)

	s.True(s.Run("Timeout packet on Ethereum", func() {
		var multicallTx []byte
		s.Require().True(s.Run("Retrieve timeout tx to Ethereum", func() {
			resp, err := s.CosmosToEthRelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
				TimeoutTxIds:    txHashes,
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
		}))

		s.Require().True(s.Run("Verify balances on Ethereum", func() {
			// User balance on Ethereum
			userBalance, err := s.erc20Contract.BalanceOf(nil, ethereumUserAddress)
			s.Require().NoError(err)
			s.Require().Equal(testvalues.InitialBalance, userBalance.Int64())

			// ICS20 contract balance on Ethereum
			escrowBalance, err := s.erc20Contract.BalanceOf(nil, s.escrowContractAddr)
			s.Require().NoError(err)
			s.Require().Equal(int64(0), escrowBalance.Int64())
		}))
	}))
}

func (s *RelayerTestSuite) TestRecvPacketToCosmos() {
	ctx := context.Background()
	s.RecvPacketToCosmosTest(ctx, 1)
}

func (s *RelayerTestSuite) Test_10_RecvPacketToCosmos() {
	ctx := context.Background()
	s.RecvPacketToCosmosTest(ctx, 10)
}

func (s *RelayerTestSuite) RecvPacketToCosmosTest(ctx context.Context, numOfTransfers int) {
	s.SetupSuite(ctx, operator.ProofTypeGroth16) // Doesn't matter, since we won't relay to eth in this test

	eth, simd := s.EthChain, s.CosmosChains[0]

	ics20Address := ethcommon.HexToAddress(s.contractAddresses.Ics20Transfer)
	transferAmount := big.NewInt(testvalues.TransferAmount)
	totalTransferAmount := big.NewInt(testvalues.TransferAmount * int64(numOfTransfers)) // total amount transferred
	if totalTransferAmount.Int64() > testvalues.InitialBalance {
		s.FailNow("Total transfer amount exceeds the initial balance")
	}
	ethereumUserAddress := crypto.PubkeyToAddress(s.key.PublicKey)
	cosmosUserWallet := s.CosmosUsers[0]
	cosmosUserAddress := cosmosUserWallet.FormattedAddress()

	ics26routerAbi, err := abi.JSON(strings.NewReader(ics26router.ContractABI))
	s.Require().NoError(err)

	s.Require().True(s.Run("Approve the ICS20Transfer.sol contract to spend the erc20 tokens", func() {
		tx, err := s.erc20Contract.Approve(s.GetTransactOpts(s.key, eth), ics20Address, totalTransferAmount)
		s.Require().NoError(err)
		receipt := s.GetTxReciept(ctx, eth, tx.Hash())
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)

		allowance, err := s.erc20Contract.Allowance(nil, ethereumUserAddress, ics20Address)
		s.Require().NoError(err)
		s.Require().Equal(totalTransferAmount, allowance)
	}))

	var sendPacket ics26router.IICS26RouterMsgsPacket

	var txHashes [][]byte
	s.Require().True(s.Run(fmt.Sprintf("Send %d transfers on Ethereum", numOfTransfers), func() {
		timeout := uint64(time.Now().Add(30 * time.Minute).Unix())
		transferMulticall := make([][]byte, numOfTransfers)

		msgSendPacket := s.createICS20MsgSendPacket(
			ethereumUserAddress,
			s.contractAddresses.Erc20,
			transferAmount,
			cosmosUserAddress,
			s.TendermintLightClientID,
			timeout,
			"",
		)

		encodedMsg, err := ics26routerAbi.Pack("sendPacket", msgSendPacket)
		s.Require().NoError(err)
		for i := 0; i < numOfTransfers; i++ {
			transferMulticall[i] = encodedMsg
		}

		tx, err := s.ics26Contract.Multicall(s.GetTransactOpts(s.key, eth), transferMulticall)
		s.Require().NoError(err)
		receipt := s.GetTxReciept(ctx, eth, tx.Hash())
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)
		s.T().Logf("Multicall send %d transfers gas used: %d", numOfTransfers, receipt.GasUsed)
		txHashes = append(txHashes, tx.Hash().Bytes())

		sendPacketEvent, err := e2esuite.GetEvmEvent(receipt, s.ics26Contract.ParseSendPacket)
		s.Require().NoError(err)
		sendPacket = sendPacketEvent.Packet
		s.Require().Equal(uint32(1), sendPacket.Sequence)
		s.Require().Equal(timeout, sendPacket.TimeoutTimestamp)
		s.Require().Len(sendPacket.Payloads, 1)
		s.Require().Equal(transfertypes.PortID, sendPacket.Payloads[0].SourcePort)
		s.Require().Equal(s.TendermintLightClientID, sendPacket.SourceChannel)
		s.Require().Equal(transfertypes.PortID, sendPacket.Payloads[0].DestPort)
		s.Require().Equal(ibctesting.FirstChannelID, sendPacket.DestChannel)
		s.Require().Equal(transfertypes.V1, sendPacket.Payloads[0].Version)
		s.Require().Equal(transfertypes.EncodingABI, sendPacket.Payloads[0].Encoding)

		s.True(s.Run("Verify balances on Ethereum", func() {
			// User balance on Ethereum
			userBalance, err := s.erc20Contract.BalanceOf(nil, ethereumUserAddress)
			s.Require().NoError(err)
			s.Require().Equal(testvalues.InitialBalance-totalTransferAmount.Int64(), userBalance.Int64())

			// ICS20 contract balance on Ethereum
			escrowBalance, err := s.erc20Contract.BalanceOf(nil, s.escrowContractAddr)
			s.Require().NoError(err)
			s.Require().Equal(totalTransferAmount, escrowBalance)
		}))
	}))

	var txBodyBz []byte
	s.Require().True(s.Run("Retrieve relay tx to Cosmos chain", func() {
		resp, err := s.EthToCosmosRelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
			SourceTxIds:     txHashes,
			TargetChannelId: ibctesting.FirstChannelID,
		})
		s.Require().NoError(err)
		s.Require().NotEmpty(resp.Tx)
		s.Require().Empty(resp.Address)

		txBodyBz = resp.Tx
	}))

	var ackTxHash []byte
	s.Require().True(s.Run("Broadcast relay tx on Cosmos chain", func() {
		var txBody txtypes.TxBody
		err := proto.Unmarshal(txBodyBz, &txBody)
		s.Require().NoError(err)

		var msgs []sdk.Msg
		for _, msg := range txBody.Messages {
			var sdkMsg sdk.Msg
			err = simd.Config().EncodingConfig.InterfaceRegistry.UnpackAny(msg, &sdkMsg)
			s.Require().NoError(err)

			msgs = append(msgs, sdkMsg)
		}

		s.Require().NotZero(len(msgs))

		resp, err := s.BroadcastMessages(ctx, simd, s.SimdSubmitter, 2_000_000, msgs...)
		s.Require().NoError(err)

		ackTxHash, err = hex.DecodeString(resp.TxHash)
		s.Require().NoError(err)
		s.Require().NotEmpty(ackTxHash)
	}))

	s.Require().True(s.Run("Verify balances on Cosmos chain", func() {
		denomOnCosmos := transfertypes.NewDenom(s.contractAddresses.Erc20, transfertypes.NewHop(transfertypes.PortID, ibctesting.FirstChannelID))

		// User balance on Cosmos chain
		resp, err := e2esuite.GRPCQuery[banktypes.QueryBalanceResponse](ctx, simd, &banktypes.QueryBalanceRequest{
			Address: cosmosUserAddress,
			Denom:   denomOnCosmos.IBCDenom(),
		})
		s.Require().NoError(err)
		s.Require().NotNil(resp.Balance)
		s.Require().Equal(totalTransferAmount.Uint64(), resp.Balance.Amount.Uint64())
		s.Require().Equal(denomOnCosmos.IBCDenom(), resp.Balance.Denom)
	}))
}
