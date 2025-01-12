package main

import (
	"context"
	"encoding/hex"
	"math/big"
	"os"
	"testing"
	"time"

	"github.com/cosmos/gogoproto/proto"
	"github.com/cosmos/solidity-ibc-eureka/abigen/ics20lib"
	"github.com/stretchr/testify/suite"

	sdkmath "cosmossdk.io/math"
	banktypes "cosmossdk.io/x/bank/types"

	codectypes "github.com/cosmos/cosmos-sdk/codec/types"
	sdk "github.com/cosmos/cosmos-sdk/types"
	txtypes "github.com/cosmos/cosmos-sdk/types/tx"

	transfertypes "github.com/cosmos/ibc-go/v9/modules/apps/transfer/types"
	clienttypes "github.com/cosmos/ibc-go/v9/modules/core/02-client/types"
	channeltypesv2 "github.com/cosmos/ibc-go/v9/modules/core/04-channel/v2/types"
	commitmenttypes "github.com/cosmos/ibc-go/v9/modules/core/23-commitment/types"
	commitmenttypesv2 "github.com/cosmos/ibc-go/v9/modules/core/23-commitment/types/v2"
	ibcexported "github.com/cosmos/ibc-go/v9/modules/core/exported"
	ibctm "github.com/cosmos/ibc-go/v9/modules/light-clients/07-tendermint"
	ibctesting "github.com/cosmos/ibc-go/v9/testing"

	"github.com/strangelove-ventures/interchaintest/v9/chain/cosmos"
	"github.com/strangelove-ventures/interchaintest/v9/ibc"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/chainconfig"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/e2esuite"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/relayer"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
	relayertypes "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/relayer"
)

// CosmosRelayerTestSuite is a struct that holds the test suite for two Cosmos chains.
type CosmosRelayerTestSuite struct {
	e2esuite.TestSuite

	SimdA *cosmos.CosmosChain
	SimdB *cosmos.CosmosChain

	SimdASubmitter ibc.Wallet
	SimdBSubmitter ibc.Wallet

	AtoBRelayerClient relayertypes.RelayerServiceClient
	BtoARelayerClient relayertypes.RelayerServiceClient
}

// TestWithIbcEurekaTestSuite is the boilerplate code that allows the test suite to be run
func TestWithCosmosRelayerTestSuite(t *testing.T) {
	suite.Run(t, new(CosmosRelayerTestSuite))
}

// SetupSuite calls the underlying IbcEurekaTestSuite's SetupSuite method
// and deploys the IbcEureka contract
func (s *CosmosRelayerTestSuite) SetupSuite(ctx context.Context) {
	chainconfig.DefaultChainSpecs = append(chainconfig.DefaultChainSpecs, chainconfig.IbcGoChainSpec("ibc-go-simd-2", "simd-2"))

	os.Setenv(testvalues.EnvKeyEthTestnetType, testvalues.EthTestnetTypeNone)

	s.TestSuite.SetupSuite(ctx)

	s.SimdA, s.SimdB = s.CosmosChains[0], s.CosmosChains[1]
	s.SimdASubmitter = s.CreateAndFundCosmosUser(ctx, s.SimdA)
	s.SimdBSubmitter = s.CreateAndFundCosmosUser(ctx, s.SimdB)

	var (
		relayerProcess *os.Process
		configInfo     relayer.CosmosToCosmosConfigInfo
	)
	s.Require().True(s.Run("Start Relayer", func() {
		err := os.Chdir("../..")
		s.Require().NoError(err)

		configInfo = relayer.CosmosToCosmosConfigInfo{
			ChainATmRPC: s.SimdA.GetHostRPCAddress(),
			ChainBTmRPC: s.SimdB.GetHostRPCAddress(),
			ChainAUser:  s.SimdASubmitter.FormattedAddress(),
			ChainBUser:  s.SimdBSubmitter.FormattedAddress(),
		}

		err = configInfo.GenerateCosmosToCosmosConfigFile(testvalues.RelayerConfigFilePath)
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

	s.Require().True(s.Run("Create Relayer Client", func() {
		var err error
		s.AtoBRelayerClient, err = relayer.GetGRPCClient(configInfo.ChainAToChainBGRPCAddress())
		s.Require().NoError(err)

		s.BtoARelayerClient, err = relayer.GetGRPCClient(configInfo.ChainBToChainAGRPCAddress())
		s.Require().NoError(err)
	}))

	s.Require().True(s.Run("Create Light Client of Chain A on Chain B", func() {
		simdAHeader, err := s.FetchCosmosHeader(ctx, s.SimdA)
		s.Require().NoError(err)

		var (
			clientStateAny    *codectypes.Any
			consensusStateAny *codectypes.Any
		)
		s.Require().True(s.Run("Construct the client and consensus state", func() {
			tmConfig := ibctesting.NewTendermintConfig()
			revision := clienttypes.ParseChainID(simdAHeader.ChainID)
			height := clienttypes.NewHeight(revision, uint64(simdAHeader.Height))

			clientState := ibctm.NewClientState(
				simdAHeader.ChainID,
				tmConfig.TrustLevel, tmConfig.TrustingPeriod, tmConfig.UnbondingPeriod, tmConfig.MaxClockDrift,
				height, commitmenttypes.GetSDKSpecs(), ibctesting.UpgradePath,
			)
			clientStateAny, err = codectypes.NewAnyWithValue(clientState)
			s.Require().NoError(err)

			consensusState := ibctm.NewConsensusState(simdAHeader.Time, commitmenttypes.NewMerkleRoot([]byte(ibctm.SentinelRoot)), simdAHeader.ValidatorsHash)
			consensusStateAny, err = codectypes.NewAnyWithValue(consensusState)
			s.Require().NoError(err)
		}))

		_, err = s.BroadcastMessages(ctx, s.SimdB, s.SimdBSubmitter, 200_000, &clienttypes.MsgCreateClient{
			ClientState:    clientStateAny,
			ConsensusState: consensusStateAny,
			Signer:         s.SimdBSubmitter.FormattedAddress(),
		})
		s.Require().NoError(err)
	}))

	s.Require().True(s.Run("Create Light Client of Chain B on Chain A", func() {
		simdBHeader, err := s.FetchCosmosHeader(ctx, s.SimdB)
		s.Require().NoError(err)

		var (
			clientStateAny    *codectypes.Any
			consensusStateAny *codectypes.Any
		)
		s.Require().True(s.Run("Construct the client and consensus state", func() {
			tmConfig := ibctesting.NewTendermintConfig()
			revision := clienttypes.ParseChainID(simdBHeader.ChainID)
			height := clienttypes.NewHeight(revision, uint64(simdBHeader.Height))

			clientState := ibctm.NewClientState(
				simdBHeader.ChainID,
				tmConfig.TrustLevel, tmConfig.TrustingPeriod, tmConfig.UnbondingPeriod, tmConfig.MaxClockDrift,
				height, commitmenttypes.GetSDKSpecs(), ibctesting.UpgradePath,
			)
			clientStateAny, err = codectypes.NewAnyWithValue(clientState)
			s.Require().NoError(err)

			consensusState := ibctm.NewConsensusState(simdBHeader.Time, commitmenttypes.NewMerkleRoot([]byte(ibctm.SentinelRoot)), simdBHeader.ValidatorsHash)
			consensusStateAny, err = codectypes.NewAnyWithValue(consensusState)
			s.Require().NoError(err)
		}))

		_, err = s.BroadcastMessages(ctx, s.SimdA, s.SimdASubmitter, 200_000, &clienttypes.MsgCreateClient{
			ClientState:    clientStateAny,
			ConsensusState: consensusStateAny,
			Signer:         s.SimdASubmitter.FormattedAddress(),
		})
		s.Require().NoError(err)
	}))

	s.Require().True(s.Run("Create Channel and register counterparty on Chain A", func() {
		merklePathPrefix := commitmenttypesv2.NewMerklePath([]byte(ibcexported.StoreKey), []byte(""))

		// We can do this because we know what the counterparty channel ID will be
		_, err := s.BroadcastMessages(ctx, s.SimdA, s.SimdASubmitter, 200_000, &channeltypesv2.MsgCreateChannel{
			ClientId:         ibctesting.FirstClientID,
			MerklePathPrefix: merklePathPrefix,
			Signer:           s.SimdASubmitter.FormattedAddress(),
		}, &channeltypesv2.MsgRegisterCounterparty{
			ChannelId:             ibctesting.FirstChannelID,
			CounterpartyChannelId: ibctesting.FirstChannelID,
			Signer:                s.SimdASubmitter.FormattedAddress(),
		})
		s.Require().NoError(err)
	}))

	s.Require().True(s.Run("Create Channel and register counterparty on Chain B", func() {
		merklePathPrefix := commitmenttypesv2.NewMerklePath([]byte(ibcexported.StoreKey), []byte(""))

		_, err := s.BroadcastMessages(ctx, s.SimdB, s.SimdBSubmitter, 200_000, &channeltypesv2.MsgCreateChannel{
			ClientId:         ibctesting.FirstClientID,
			MerklePathPrefix: merklePathPrefix,
			Signer:           s.SimdBSubmitter.FormattedAddress(),
		}, &channeltypesv2.MsgRegisterCounterparty{
			ChannelId:             ibctesting.FirstChannelID,
			CounterpartyChannelId: ibctesting.FirstChannelID,
			Signer:                s.SimdBSubmitter.FormattedAddress(),
		})
		s.Require().NoError(err)
	}))
}

// TestRelayer is a test that runs the relayer
func (s *CosmosRelayerTestSuite) TestRelayerInfo() {
	ctx := context.Background()
	s.SetupSuite(ctx)

	s.Run("Chain A to B Relayer Info", func() {
		info, err := s.AtoBRelayerClient.Info(context.Background(), &relayertypes.InfoRequest{})
		s.Require().NoError(err)
		s.Require().NotNil(info)

		s.T().Logf("Relayer Info: %+v", info)

		s.Require().Equal(s.SimdA.Config().ChainID, info.SourceChain.ChainId)
		s.Require().Equal(s.SimdB.Config().ChainID, info.TargetChain.ChainId)
	})

	s.Run("Chain B to A Relayer Info", func() {
		info, err := s.BtoARelayerClient.Info(context.Background(), &relayertypes.InfoRequest{})
		s.Require().NoError(err)
		s.Require().NotNil(info)

		s.T().Logf("Relayer Info: %+v", info)

		s.Require().Equal(s.SimdB.Config().ChainID, info.SourceChain.ChainId)
		s.Require().Equal(s.SimdA.Config().ChainID, info.TargetChain.ChainId)
	})
}

func (s *CosmosRelayerTestSuite) TestICS20RecvAndAckPacket() {
	ctx := context.Background()
	s.ICS20RecvAndAckPacketTest(ctx, 1)
}

func (s *CosmosRelayerTestSuite) Test_10_ICS20RecvAndAckPacket() {
	ctx := context.Background()
	s.ICS20RecvAndAckPacketTest(ctx, 10)
}

func (s *CosmosRelayerTestSuite) ICS20RecvAndAckPacketTest(ctx context.Context, numOfTransfers int) {
	s.Require().Greater(numOfTransfers, 0)

	s.SetupSuite(ctx)

	simdAUser, simdBUser := s.CosmosUsers[0], s.CosmosUsers[1]
	transferAmount := big.NewInt(testvalues.TransferAmount)
	totalTransferAmount := testvalues.TransferAmount * int64(numOfTransfers)

	var txHashes [][]byte
	s.Require().True(s.Run("Send transfers on Chain A", func() {
		for i := 0; i < numOfTransfers; i++ {
			timeout := uint64(time.Now().Add(30 * time.Minute).Unix())
			transferCoin := sdk.NewCoin(s.SimdA.Config().Denom, sdkmath.NewIntFromBigInt(transferAmount))

			transferPayload := ics20lib.ICS20LibFungibleTokenPacketData{
				Denom:    transferCoin.Denom,
				Amount:   transferCoin.Amount.BigInt(),
				Sender:   simdAUser.FormattedAddress(),
				Receiver: simdBUser.FormattedAddress(),
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
				Signer: simdAUser.FormattedAddress(),
			}

			resp, err := s.BroadcastMessages(ctx, s.SimdA, simdAUser, 200_000, &msgSendPacket)
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.TxHash)

			txHash, err := hex.DecodeString(resp.TxHash)
			s.Require().NoError(err)
			s.Require().NotEmpty(txHash)

			txHashes = append(txHashes, txHash)
		}

		s.Require().True(s.Run("Verify balances on Cosmos chain", func() {
			resp, err := e2esuite.GRPCQuery[banktypes.QueryBalanceResponse](ctx, s.SimdA, &banktypes.QueryBalanceRequest{
				Address: simdAUser.FormattedAddress(),
				Denom:   s.SimdA.Config().Denom,
			})
			s.Require().NoError(err)
			s.Require().NotNil(resp.Balance)
			s.Require().Equal(testvalues.InitialBalance-totalTransferAmount, resp.Balance.Amount.Int64())
		}))
	}))

	var txBodyBz []byte
	s.Require().True(s.Run("Retrieve relay tx to Chain B", func() {
		resp, err := s.AtoBRelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
			SourceTxIds:     txHashes,
			TargetChannelId: ibctesting.FirstChannelID,
		})
		s.Require().NoError(err)
		s.Require().NotEmpty(resp.Tx)
		s.Require().Empty(resp.Address)

		txBodyBz = resp.Tx
	}))

	var ackTxHash []byte
	s.Require().True(s.Run("Broadcast relay tx on Chain B", func() {
		var txBody txtypes.TxBody
		err := proto.Unmarshal(txBodyBz, &txBody)
		s.Require().NoError(err)

		var msgs []sdk.Msg
		for _, msg := range txBody.Messages {
			var sdkMsg sdk.Msg
			err = s.SimdB.Config().EncodingConfig.InterfaceRegistry.UnpackAny(msg, &sdkMsg)
			s.Require().NoError(err)

			msgs = append(msgs, sdkMsg)
		}

		resp, err := s.BroadcastMessages(ctx, s.SimdB, s.SimdBSubmitter, 2_000_000, msgs...)
		s.Require().NoError(err)

		ackTxHash, err = hex.DecodeString(resp.TxHash)
		s.Require().NoError(err)
		s.Require().NotEmpty(ackTxHash)

		s.Require().True(s.Run("Verify balances on Cosmos chain", func() {
			ibcDenom := transfertypes.NewDenom(s.SimdA.Config().Denom, transfertypes.NewHop(transfertypes.PortID, ibctesting.FirstChannelID)).IBCDenom()
			// User balance on Cosmos chain
			resp, err := e2esuite.GRPCQuery[banktypes.QueryBalanceResponse](ctx, s.SimdB, &banktypes.QueryBalanceRequest{
				Address: simdBUser.FormattedAddress(),
				Denom:   ibcDenom,
			})
			s.Require().NoError(err)
			s.Require().NotNil(resp.Balance)
			s.Require().Equal(totalTransferAmount, resp.Balance.Amount.Int64())
			s.Require().Equal(ibcDenom, resp.Balance.Denom)
		}))
	}))

	var ackTxBodyBz []byte
	s.Require().True(s.Run("Retrieve ack tx to Chain A", func() {
		resp, err := s.BtoARelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
			SourceTxIds:     [][]byte{ackTxHash},
			TargetChannelId: ibctesting.FirstChannelID,
		})
		s.Require().NoError(err)
		s.Require().NotEmpty(resp.Tx)
		s.Require().Empty(resp.Address)

		ackTxBodyBz = resp.Tx
	}))

	s.Require().True(s.Run("Broadcast ack tx on Chain A", func() {
		var txBody txtypes.TxBody
		err := proto.Unmarshal(ackTxBodyBz, &txBody)
		s.Require().NoError(err)

		var msgs []sdk.Msg
		for _, msg := range txBody.Messages {
			var sdkMsg sdk.Msg
			err = s.SimdA.Config().EncodingConfig.InterfaceRegistry.UnpackAny(msg, &sdkMsg)
			s.Require().NoError(err)

			msgs = append(msgs, sdkMsg)
		}

		_, err = s.BroadcastMessages(ctx, s.SimdA, s.SimdASubmitter, 2_000_000, msgs...)
		s.Require().NoError(err)

		s.Require().True(s.Run("Verify commitments removed", func() {
			for i := 0; i < numOfTransfers; i++ {
				_, err := e2esuite.GRPCQuery[channeltypesv2.QueryPacketCommitmentResponse](ctx, s.SimdA, &channeltypesv2.QueryPacketCommitmentRequest{
					ChannelId: ibctesting.FirstChannelID,
					Sequence:  uint64(i) + 1,
				})
				s.Require().ErrorContains(err, "packet commitment hash not found")
			}
		}))
	}))
}

func (s *CosmosRelayerTestSuite) TestICS20TimeoutPacket() {
	ctx := context.Background()
	s.ICS20TimeoutPacketTest(ctx, 1)
}

func (s *CosmosRelayerTestSuite) Test_10_ICS20TimeoutPacket() {
	ctx := context.Background()
	s.ICS20TimeoutPacketTest(ctx, 10)
}

func (s *CosmosRelayerTestSuite) ICS20TimeoutPacketTest(ctx context.Context, numOfTransfers int) {
	s.Require().Greater(numOfTransfers, 0)

	s.SetupSuite(ctx)

	simdAUser, simdBUser := s.CosmosUsers[0], s.CosmosUsers[1]
	transferAmount := big.NewInt(testvalues.TransferAmount)
	totalTransferAmount := testvalues.TransferAmount * int64(numOfTransfers)

	var txHashes [][]byte
	s.Require().True(s.Run("Send transfers on Chain A", func() {
		for i := 0; i < numOfTransfers; i++ {
			timeout := uint64(time.Now().Add(30 * time.Second).Unix())
			transferCoin := sdk.NewCoin(s.SimdA.Config().Denom, sdkmath.NewIntFromBigInt(transferAmount))

			transferPayload := ics20lib.ICS20LibFungibleTokenPacketData{
				Denom:    transferCoin.Denom,
				Amount:   transferCoin.Amount.BigInt(),
				Sender:   simdAUser.FormattedAddress(),
				Receiver: simdBUser.FormattedAddress(),
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
				Signer: simdAUser.FormattedAddress(),
			}

			resp, err := s.BroadcastMessages(ctx, s.SimdA, simdAUser, 200_000, &msgSendPacket)
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.TxHash)

			txHash, err := hex.DecodeString(resp.TxHash)
			s.Require().NoError(err)
			s.Require().NotEmpty(txHash)

			txHashes = append(txHashes, txHash)
		}

		s.Require().True(s.Run("Verify balances on Cosmos chain", func() {
			resp, err := e2esuite.GRPCQuery[banktypes.QueryBalanceResponse](ctx, s.SimdA, &banktypes.QueryBalanceRequest{
				Address: simdAUser.FormattedAddress(),
				Denom:   s.SimdA.Config().Denom,
			})
			s.Require().NoError(err)
			s.Require().NotNil(resp.Balance)
			s.Require().Equal(testvalues.InitialBalance-totalTransferAmount, resp.Balance.Amount.Int64())
		}))
	}))

	// Wait until timeout
	time.Sleep(30 * time.Second)

	var timeoutTxBodyBz []byte
	s.Require().True(s.Run("Retrieve timeout tx to Chain A", func() {
		resp, err := s.BtoARelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
			TimeoutTxIds:    txHashes,
			TargetChannelId: ibctesting.FirstChannelID,
		})
		s.Require().NoError(err)
		s.Require().NotEmpty(resp.Tx)
		s.Require().Empty(resp.Address)

		timeoutTxBodyBz = resp.Tx
	}))

	s.Require().True(s.Run("Broadcast timeout tx on Chain A", func() {
		var txBody txtypes.TxBody
		err := proto.Unmarshal(timeoutTxBodyBz, &txBody)
		s.Require().NoError(err)

		var msgs []sdk.Msg
		for _, msg := range txBody.Messages {
			var sdkMsg sdk.Msg
			err = s.SimdA.Config().EncodingConfig.InterfaceRegistry.UnpackAny(msg, &sdkMsg)
			s.Require().NoError(err)

			msgs = append(msgs, sdkMsg)
		}

		_, err = s.BroadcastMessages(ctx, s.SimdA, s.SimdASubmitter, 2_000_000, msgs...)
		s.Require().NoError(err)

		s.Require().True(s.Run("Verify balances on Cosmos chain", func() {
			resp, err := e2esuite.GRPCQuery[banktypes.QueryBalanceResponse](ctx, s.SimdA, &banktypes.QueryBalanceRequest{
				Address: simdAUser.FormattedAddress(),
				Denom:   s.SimdA.Config().Denom,
			})
			s.Require().NoError(err)
			s.Require().NotNil(resp.Balance)
			s.Require().Equal(testvalues.InitialBalance, resp.Balance.Amount.Int64())
		}))
	}))
}
