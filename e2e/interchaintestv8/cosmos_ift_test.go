package main

import (
	"context"
	"encoding/hex"
	"os"
	"testing"
	"time"

	"github.com/stretchr/testify/suite"

	sdkmath "cosmossdk.io/math"

	sdk "github.com/cosmos/cosmos-sdk/types"
	authtypes "github.com/cosmos/cosmos-sdk/x/auth/types"
	banktypes "github.com/cosmos/cosmos-sdk/x/bank/types"
	govtypes "github.com/cosmos/cosmos-sdk/x/gov/types"

	clienttypes "github.com/cosmos/ibc-go/v10/modules/core/02-client/types"
	clienttypesv2 "github.com/cosmos/ibc-go/v10/modules/core/02-client/v2/types"
	ibcexported "github.com/cosmos/ibc-go/v10/modules/core/exported"
	ibctesting "github.com/cosmos/ibc-go/v10/testing"

	interchaintest "github.com/cosmos/interchaintest/v10"
	"github.com/cosmos/interchaintest/v10/chain/cosmos"
	"github.com/cosmos/interchaintest/v10/ibc"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/chainconfig"
	cosmosutils "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/cosmos"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/e2esuite"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/relayer"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
	relayertypes "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/relayer"
	ifttypes "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/wfchain/ift"
	tokenfactorytypes "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/wfchain/tokenfactory"
)

const (
	testIFTDenom              = "factory/wf1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq5a9p63/testift"
	iftSendCallConstructorCtx = "cosmos"
	iftModuleName             = "ift"
)

// CosmosIFTTestSuite tests IFT transfers between two wfchain instances
type CosmosIFTTestSuite struct {
	e2esuite.TestSuite

	ChainA *cosmos.CosmosChain
	ChainB *cosmos.CosmosChain

	ChainASubmitter ibc.Wallet
	ChainBSubmitter ibc.Wallet

	RelayerClient relayertypes.RelayerServiceClient
}

func TestWithCosmosIFTTestSuite(t *testing.T) {
	suite.Run(t, new(CosmosIFTTestSuite))
}

func (s *CosmosIFTTestSuite) SetupSuite(ctx context.Context) {
	// Use two wfchain instances
	chainconfig.DefaultChainSpecs = []*interchaintest.ChainSpec{
		chainconfig.WfchainChainSpec("wfchain-1", "wfchain-1"),
		chainconfig.WfchainChainSpec("wfchain-2", "wfchain-2"),
	}

	os.Setenv(testvalues.EnvKeyEthTestnetType, testvalues.EthTestnetTypeNone)
	os.Setenv(testvalues.EnvKeySolanaTestnetType, testvalues.SolanaTestnetType_None)

	s.TestSuite.SetupSuite(ctx)

	s.ChainA, s.ChainB = s.CosmosChains[0], s.CosmosChains[1]
	s.ChainASubmitter = s.CreateAndFundCosmosUser(ctx, s.ChainA)
	s.ChainBSubmitter = s.CreateAndFundCosmosUser(ctx, s.ChainB)

	var relayerProcess *os.Process
	s.Require().True(s.Run("Start Relayer", func() {
		err := os.Chdir("../..")
		s.Require().NoError(err)

		config := relayer.NewConfig(
			relayer.CreateCosmosCosmosModules(relayer.CosmosToCosmosConfigInfo{
				ChainAID:    s.ChainA.Config().ChainID,
				ChainBID:    s.ChainB.Config().ChainID,
				ChainATmRPC: s.ChainA.GetHostRPCAddress(),
				ChainBTmRPC: s.ChainB.GetHostRPCAddress(),
				ChainAUser:  s.ChainASubmitter.FormattedAddress(),
				ChainBUser:  s.ChainBSubmitter.FormattedAddress(),
			}),
		)

		err = config.GenerateConfigFile(testvalues.RelayerConfigFilePath)
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
		s.RelayerClient, err = relayer.GetGRPCClient(relayer.DefaultRelayerGRPCAddress())
		s.Require().NoError(err)
	}))
}

func (s *CosmosIFTTestSuite) createLightClients(ctx context.Context) {
	s.Require().True(s.Run("Create Light Client of Chain A on Chain B", func() {
		var createClientTxBodyBz []byte
		s.Require().True(s.Run("Retrieve create client tx", func() {
			resp, err := s.RelayerClient.CreateClient(context.Background(), &relayertypes.CreateClientRequest{
				SrcChain: s.ChainA.Config().ChainID,
				DstChain: s.ChainB.Config().ChainID,
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Tx)

			createClientTxBodyBz = resp.Tx
		}))

		s.Require().True(s.Run("Broadcast create client tx", func() {
			resp := s.MustBroadcastSdkTxBody(ctx, s.ChainB, s.ChainBSubmitter, 2_000_000, createClientTxBodyBz)
			clientId, err := cosmosutils.GetEventValue(resp.Events, clienttypes.EventTypeCreateClient, clienttypes.AttributeKeyClientID)
			s.Require().NoError(err)
			s.Require().Equal(ibctesting.FirstClientID, clientId)
		}))
	}))

	s.Require().True(s.Run("Create Light Client of Chain B on Chain A", func() {
		var createClientTxBodyBz []byte
		s.Require().True(s.Run("Retrieve create client tx", func() {
			resp, err := s.RelayerClient.CreateClient(context.Background(), &relayertypes.CreateClientRequest{
				SrcChain: s.ChainB.Config().ChainID,
				DstChain: s.ChainA.Config().ChainID,
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Tx)

			createClientTxBodyBz = resp.Tx
		}))

		s.Require().True(s.Run("Broadcast create client tx", func() {
			resp := s.MustBroadcastSdkTxBody(ctx, s.ChainA, s.ChainASubmitter, 2_000_000, createClientTxBodyBz)
			clientId, err := cosmosutils.GetEventValue(resp.Events, clienttypes.EventTypeCreateClient, clienttypes.AttributeKeyClientID)
			s.Require().NoError(err)
			s.Require().Equal(ibctesting.FirstClientID, clientId)
		}))
	}))

	s.Require().True(s.Run("Register counterparty on Chain A", func() {
		merklePathPrefix := [][]byte{[]byte(ibcexported.StoreKey), []byte("")}

		_, err := s.BroadcastMessages(ctx, s.ChainA, s.ChainASubmitter, 200_000, &clienttypesv2.MsgRegisterCounterparty{
			ClientId:                 ibctesting.FirstClientID,
			CounterpartyClientId:     ibctesting.FirstClientID,
			CounterpartyMerklePrefix: merklePathPrefix,
			Signer:                   s.ChainASubmitter.FormattedAddress(),
		})
		s.Require().NoError(err)
	}))

	s.Require().True(s.Run("Register counterparty on Chain B", func() {
		merklePathPrefix := [][]byte{[]byte(ibcexported.StoreKey), []byte("")}

		_, err := s.BroadcastMessages(ctx, s.ChainB, s.ChainBSubmitter, 200_000, &clienttypesv2.MsgRegisterCounterparty{
			ClientId:                 ibctesting.FirstClientID,
			CounterpartyClientId:     ibctesting.FirstClientID,
			CounterpartyMerklePrefix: merklePathPrefix,
			Signer:                   s.ChainBSubmitter.FormattedAddress(),
		})
		s.Require().NoError(err)
	}))
}

// createTokenFactoryDenom creates a denom using the tokenfactory module
// Note: wfchain tokenfactory uses simple denoms (max 20 chars alphanumeric), not factory/creator/subdenom format
func (s *CosmosIFTTestSuite) createTokenFactoryDenom(ctx context.Context, chain *cosmos.CosmosChain, user ibc.Wallet, denom string) string {
	msg := &tokenfactorytypes.MsgCreateDenom{
		Sender: user.FormattedAddress(),
		Denom:  denom,
	}

	_, err := s.BroadcastMessages(ctx, chain, user, 200_000, msg)
	s.Require().NoError(err)

	return denom
}

// mintTokens mints tokens using the tokenfactory module
func (s *CosmosIFTTestSuite) mintTokens(ctx context.Context, chain *cosmos.CosmosChain, user ibc.Wallet, denom string, amount sdkmath.Int, recipient string) {
	msg := &tokenfactorytypes.MsgMint{
		From:    user.FormattedAddress(),
		Address: recipient,
		Amount:  sdk.Coin{Denom: denom, Amount: amount},
	}

	_, err := s.BroadcastMessages(ctx, chain, user, 200_000, msg)
	s.Require().NoError(err)
}

// registerIFTBridge registers an IFT bridge via governance proposal
func (s *CosmosIFTTestSuite) registerIFTBridge(ctx context.Context, chain *cosmos.CosmosChain, user ibc.Wallet, denom, clientId, counterpartyIftAddr, constructor string) {
	// Get governance module address for the signer
	govModuleAddr, err := chain.AuthQueryModuleAddress(ctx, govtypes.ModuleName)
	s.Require().NoError(err)

	msg := &ifttypes.MsgRegisterIFTBridge{
		Signer:                 govModuleAddr,
		Denom:                  denom,
		ClientId:               clientId,
		CounterpartyIftAddress: counterpartyIftAddr,
		IftSendCallConstructor: constructor,
	}

	err = s.ExecuteGovV1Proposal(ctx, msg, chain, user)
	s.Require().NoError(err)
}

// iftTransfer initiates an IFT transfer
func (s *CosmosIFTTestSuite) iftTransfer(ctx context.Context, chain *cosmos.CosmosChain, user ibc.Wallet, denom, clientId, receiver string, amount sdkmath.Int, timeoutTimestamp uint64) string {
	msg := &ifttypes.MsgIFTTransfer{
		Signer:           user.FormattedAddress(),
		Denom:            denom,
		ClientId:         clientId,
		Receiver:         receiver,
		Amount:           amount,
		TimeoutTimestamp: timeoutTimestamp,
	}

	resp, err := s.BroadcastMessages(ctx, chain, user, 200_000, msg)
	s.Require().NoError(err)

	return resp.TxHash
}

// queryBalance queries the balance of an address using gRPC
func (s *CosmosIFTTestSuite) queryBalance(ctx context.Context, chain *cosmos.CosmosChain, address, denom string) sdkmath.Int {
	resp, err := e2esuite.GRPCQuery[banktypes.QueryBalanceResponse](ctx, chain, &banktypes.QueryBalanceRequest{
		Address: address,
		Denom:   denom,
	})
	s.Require().NoError(err)

	return resp.Balance.Amount
}

func (s *CosmosIFTTestSuite) queryPendingTransfer(ctx context.Context, chain *cosmos.CosmosChain, denom, clientID string, sequence uint64) (*ifttypes.QueryPendingTransferResponse, error) {
	return e2esuite.GRPCQuery[ifttypes.QueryPendingTransferResponse](ctx, chain, &ifttypes.QueryPendingTransferRequest{
		Denom:    denom,
		ClientId: clientID,
		Sequence: sequence,
	})
}

// getIFTModuleAddress returns the IFT module address
func (s *CosmosIFTTestSuite) getIFTModuleAddress(ctx context.Context, chain *cosmos.CosmosChain) string {
	// The IFT module address is derived from the module name "ift"
	iftAddr := authtypes.NewModuleAddress(iftModuleName)

	// Convert to bech32 with chain's prefix
	bech32Addr, err := sdk.Bech32ifyAddressBytes(chain.Config().Bech32Prefix, iftAddr)
	s.Require().NoError(err)

	return bech32Addr
}

// Test_Deploy verifies that two wfchain instances start correctly
func (s *CosmosIFTTestSuite) Test_Deploy() {
	ctx := context.Background()
	s.SetupSuite(ctx)

	s.Require().True(s.Run("Verify Chain A is running", func() {
		height, err := s.ChainA.Height(ctx)
		s.Require().NoError(err)
		s.Require().Greater(height, int64(0))
		s.T().Logf("Chain A height: %d", height)
	}))

	s.Require().True(s.Run("Verify Chain B is running", func() {
		height, err := s.ChainB.Height(ctx)
		s.Require().NoError(err)
		s.Require().Greater(height, int64(0))
		s.T().Logf("Chain B height: %d", height)
	}))
}

// Test_IFTTransfer tests a roundtrip IFT transfer: A -> B -> A
func (s *CosmosIFTTestSuite) Test_IFTTransfer() {
	ctx := context.Background()
	s.SetupSuite(ctx)
	s.createLightClients(ctx)

	userA := s.CosmosUsers[0]
	userB := s.CosmosUsers[1]
	transferAmount := sdkmath.NewInt(1_000_000)
	subdenom := "testift"

	var denomA, denomB string

	s.Require().True(s.Run("Create denom on Chain A", func() {
		denomA = s.createTokenFactoryDenom(ctx, s.ChainA, s.ChainASubmitter, subdenom)
		s.T().Logf("Created denom on Chain A: %s", denomA)
	}))

	s.Require().True(s.Run("Create denom on Chain B", func() {
		denomB = s.createTokenFactoryDenom(ctx, s.ChainB, s.ChainBSubmitter, subdenom)
		s.T().Logf("Created denom on Chain B: %s", denomB)
	}))

	var iftModuleAddrA, iftModuleAddrB string
	s.Require().True(s.Run("Get IFT module addresses", func() {
		iftModuleAddrA = s.getIFTModuleAddress(ctx, s.ChainA)
		iftModuleAddrB = s.getIFTModuleAddress(ctx, s.ChainB)
		s.T().Logf("IFT module address on Chain A: %s", iftModuleAddrA)
		s.T().Logf("IFT module address on Chain B: %s", iftModuleAddrB)
	}))

	s.Require().True(s.Run("Register IFT bridge on Chain A", func() {
		s.registerIFTBridge(ctx, s.ChainA, s.ChainASubmitter, denomA, ibctesting.FirstClientID, iftModuleAddrB, iftSendCallConstructorCtx)
	}))

	s.Require().True(s.Run("Register IFT bridge on Chain B", func() {
		s.registerIFTBridge(ctx, s.ChainB, s.ChainBSubmitter, denomB, ibctesting.FirstClientID, iftModuleAddrA, iftSendCallConstructorCtx)
	}))

	s.Require().True(s.Run("Mint tokens to user on Chain A", func() {
		s.mintTokens(ctx, s.ChainA, s.ChainASubmitter, denomA, transferAmount, userA.FormattedAddress())
	}))

	s.Require().True(s.Run("Verify initial balance on Chain A", func() {
		balance := s.queryBalance(ctx, s.ChainA, userA.FormattedAddress(), denomA)
		s.Require().True(balance.Equal(transferAmount), "expected %s, got %s", transferAmount, balance)
		s.T().Logf("User balance on Chain A: %s", balance)
	}))

	var ackTxHash []byte
	s.Require().True(s.Run("Transfer A to B", func() {
		var sendTxHash string
		s.Require().True(s.Run("Execute IFT transfer", func() {
			timeout := uint64(time.Now().Add(30 * time.Minute).Unix())
			sendTxHash = s.iftTransfer(ctx, s.ChainA, userA, denomA, ibctesting.FirstClientID, userB.FormattedAddress(), transferAmount, timeout)
			s.Require().NotEmpty(sendTxHash)
			s.T().Logf("IFT Transfer tx hash: %s", sendTxHash)
		}))

		s.Require().True(s.Run("Verify balance burned on Chain A", func() {
			balance := s.queryBalance(ctx, s.ChainA, userA.FormattedAddress(), denomA)
			s.Require().True(balance.IsZero(), "expected 0, got %s", balance)
		}))

		s.Require().True(s.Run("Relay packet to Chain B", func() {
			sendTxHashBytes, err := hex.DecodeString(sendTxHash)
			s.Require().NoError(err)

			resp, err := s.RelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
				SrcChain:    s.ChainA.Config().ChainID,
				DstChain:    s.ChainB.Config().ChainID,
				SourceTxIds: [][]byte{sendTxHashBytes},
				SrcClientId: ibctesting.FirstClientID,
				DstClientId: ibctesting.FirstClientID,
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Tx)

			broadcastResp := s.MustBroadcastSdkTxBody(ctx, s.ChainB, s.ChainBSubmitter, 2_000_000, resp.Tx)
			ackTxHash, err = hex.DecodeString(broadcastResp.TxHash)
			s.Require().NoError(err)
		}))

		s.Require().True(s.Run("Verify balance minted on Chain B", func() {
			balance := s.queryBalance(ctx, s.ChainB, userB.FormattedAddress(), denomB)
			s.Require().True(balance.Equal(transferAmount), "expected %s, got %s", transferAmount, balance)
		}))

		s.Require().True(s.Run("Verify pending transfer exists before ack", func() {
			resp, err := s.queryPendingTransfer(ctx, s.ChainA, denomA, ibctesting.FirstClientID, 1)
			s.Require().NoError(err)
			s.Require().Equal(userA.FormattedAddress(), resp.PendingTransfer.Sender)
			s.Require().Equal(transferAmount.String(), resp.PendingTransfer.Amount.String())
			s.T().Logf("Pending transfer exists: sender=%s, amount=%s", resp.PendingTransfer.Sender, resp.PendingTransfer.Amount)
		}))

		s.Require().True(s.Run("Relay acknowledgement to Chain A", func() {
			resp, err := s.RelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
				SrcChain:    s.ChainB.Config().ChainID,
				DstChain:    s.ChainA.Config().ChainID,
				SourceTxIds: [][]byte{ackTxHash},
				SrcClientId: ibctesting.FirstClientID,
				DstClientId: ibctesting.FirstClientID,
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Tx)

			_ = s.MustBroadcastSdkTxBody(ctx, s.ChainA, s.ChainASubmitter, 2_000_000, resp.Tx)
		}))

		s.Require().True(s.Run("Verify pending transfer removed after ack", func() {
			_, err := s.queryPendingTransfer(ctx, s.ChainA, denomA, ibctesting.FirstClientID, 1)
			s.Require().Error(err, "pending transfer should be removed after ack")
			s.T().Logf("Pending transfer removed as expected: %v", err)
		}))

		s.Require().True(s.Run("Verify final balances", func() {
			balanceA := s.queryBalance(ctx, s.ChainA, userA.FormattedAddress(), denomA)
			balanceB := s.queryBalance(ctx, s.ChainB, userB.FormattedAddress(), denomB)
			s.Require().True(balanceA.IsZero(), "userA should have 0, got %s", balanceA)
			s.Require().True(balanceB.Equal(transferAmount), "userB should have %s, got %s", transferAmount, balanceB)
			s.T().Logf("After A->B: userA=%s, userB=%s", balanceA, balanceB)
		}))
	}))

	s.Require().True(s.Run("Transfer B to A", func() {
		var sendTxHash string
		var ackTxHashB []byte

		s.Require().True(s.Run("Execute IFT transfer", func() {
			timeout := uint64(time.Now().Add(30 * time.Minute).Unix())
			sendTxHash = s.iftTransfer(ctx, s.ChainB, userB, denomB, ibctesting.FirstClientID, userA.FormattedAddress(), transferAmount, timeout)
			s.Require().NotEmpty(sendTxHash)
			s.T().Logf("IFT Transfer tx hash: %s", sendTxHash)
		}))

		s.Require().True(s.Run("Verify balance burned on Chain B", func() {
			balance := s.queryBalance(ctx, s.ChainB, userB.FormattedAddress(), denomB)
			s.Require().True(balance.IsZero(), "expected 0, got %s", balance)
		}))

		s.Require().True(s.Run("Relay packet to Chain A", func() {
			sendTxHashBytes, err := hex.DecodeString(sendTxHash)
			s.Require().NoError(err)

			resp, err := s.RelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
				SrcChain:    s.ChainB.Config().ChainID,
				DstChain:    s.ChainA.Config().ChainID,
				SourceTxIds: [][]byte{sendTxHashBytes},
				SrcClientId: ibctesting.FirstClientID,
				DstClientId: ibctesting.FirstClientID,
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Tx)

			broadcastResp := s.MustBroadcastSdkTxBody(ctx, s.ChainA, s.ChainASubmitter, 2_000_000, resp.Tx)
			ackTxHashB, err = hex.DecodeString(broadcastResp.TxHash)
			s.Require().NoError(err)
		}))

		s.Require().True(s.Run("Verify balance minted on Chain A", func() {
			balance := s.queryBalance(ctx, s.ChainA, userA.FormattedAddress(), denomA)
			s.Require().True(balance.Equal(transferAmount), "expected %s, got %s", transferAmount, balance)
		}))

		s.Require().True(s.Run("Verify pending transfer exists before ack", func() {
			resp, err := s.queryPendingTransfer(ctx, s.ChainB, denomB, ibctesting.FirstClientID, 1)
			s.Require().NoError(err)
			s.Require().Equal(userB.FormattedAddress(), resp.PendingTransfer.Sender)
			s.Require().Equal(transferAmount.String(), resp.PendingTransfer.Amount.String())
			s.T().Logf("Pending transfer exists: sender=%s, amount=%s", resp.PendingTransfer.Sender, resp.PendingTransfer.Amount)
		}))

		s.Require().True(s.Run("Relay acknowledgement to Chain B", func() {
			resp, err := s.RelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
				SrcChain:    s.ChainA.Config().ChainID,
				DstChain:    s.ChainB.Config().ChainID,
				SourceTxIds: [][]byte{ackTxHashB},
				SrcClientId: ibctesting.FirstClientID,
				DstClientId: ibctesting.FirstClientID,
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Tx)

			_ = s.MustBroadcastSdkTxBody(ctx, s.ChainB, s.ChainBSubmitter, 2_000_000, resp.Tx)
		}))

		s.Require().True(s.Run("Verify pending transfer removed after ack", func() {
			_, err := s.queryPendingTransfer(ctx, s.ChainB, denomB, ibctesting.FirstClientID, 1)
			s.Require().Error(err, "pending transfer should be removed after ack")
			s.T().Logf("Pending transfer removed as expected: %v", err)
		}))

		s.Require().True(s.Run("Verify final balances", func() {
			balanceA := s.queryBalance(ctx, s.ChainA, userA.FormattedAddress(), denomA)
			balanceB := s.queryBalance(ctx, s.ChainB, userB.FormattedAddress(), denomB)
			s.Require().True(balanceA.Equal(transferAmount), "userA should have %s, got %s", transferAmount, balanceA)
			s.Require().True(balanceB.IsZero(), "userB should have 0, got %s", balanceB)
			s.T().Logf("After B->A: userA=%s, userB=%s", balanceA, balanceB)
		}))
	}))
}

// Test_IFTTransferTimeout tests that a timed-out transfer refunds tokens to the sender
func (s *CosmosIFTTestSuite) Test_IFTTransferTimeout() {
	ctx := context.Background()
	s.SetupSuite(ctx)
	s.createLightClients(ctx)

	userA := s.CosmosUsers[0]
	transferAmount := sdkmath.NewInt(1_000_000)
	subdenom := "testift"

	var denomA, denomB string

	s.Require().True(s.Run("Create denom on Chain A", func() {
		denomA = s.createTokenFactoryDenom(ctx, s.ChainA, s.ChainASubmitter, subdenom)
		s.T().Logf("Created denom on Chain A: %s", denomA)
	}))

	s.Require().True(s.Run("Create denom on Chain B", func() {
		denomB = s.createTokenFactoryDenom(ctx, s.ChainB, s.ChainBSubmitter, subdenom)
		s.T().Logf("Created denom on Chain B: %s", denomB)
	}))

	var iftModuleAddrA, iftModuleAddrB string
	s.Require().True(s.Run("Get IFT module addresses", func() {
		iftModuleAddrA = s.getIFTModuleAddress(ctx, s.ChainA)
		iftModuleAddrB = s.getIFTModuleAddress(ctx, s.ChainB)
		s.T().Logf("IFT module address on Chain A: %s", iftModuleAddrA)
		s.T().Logf("IFT module address on Chain B: %s", iftModuleAddrB)
	}))

	s.Require().True(s.Run("Register IFT bridge on Chain A", func() {
		s.registerIFTBridge(ctx, s.ChainA, s.ChainASubmitter, denomA, ibctesting.FirstClientID, iftModuleAddrB, iftSendCallConstructorCtx)
	}))

	s.Require().True(s.Run("Register IFT bridge on Chain B", func() {
		s.registerIFTBridge(ctx, s.ChainB, s.ChainBSubmitter, denomB, ibctesting.FirstClientID, iftModuleAddrA, iftSendCallConstructorCtx)
	}))

	s.Require().True(s.Run("Mint tokens to user on Chain A", func() {
		s.mintTokens(ctx, s.ChainA, s.ChainASubmitter, denomA, transferAmount, userA.FormattedAddress())
	}))

	s.Require().True(s.Run("Verify initial balance on Chain A", func() {
		balance := s.queryBalance(ctx, s.ChainA, userA.FormattedAddress(), denomA)
		s.Require().True(balance.Equal(transferAmount), "expected %s, got %s", transferAmount, balance)
		s.T().Logf("User balance on Chain A: %s", balance)
	}))

	var sendTxHash string
	s.Require().True(s.Run("Send transfer with short timeout", func() {
		// Use 30 seconds to give enough time for tx confirmation and prefetch before timeout
		timeout := uint64(time.Now().Add(30 * time.Second).Unix())
		sendTxHash = s.iftTransfer(ctx, s.ChainA, userA, denomA, ibctesting.FirstClientID, userA.FormattedAddress(), transferAmount, timeout)
		s.Require().NotEmpty(sendTxHash)
		s.T().Logf("IFT Transfer tx hash: %s", sendTxHash)
	}))

	s.Require().True(s.Run("Verify balance burned on Chain A", func() {
		balance := s.queryBalance(ctx, s.ChainA, userA.FormattedAddress(), denomA)
		s.Require().True(balance.IsZero(), "expected 0, got %s", balance)
	}))

	s.Require().True(s.Run("Verify pending transfer exists", func() {
		resp, err := s.queryPendingTransfer(ctx, s.ChainA, denomA, ibctesting.FirstClientID, 1)
		s.Require().NoError(err)
		s.Require().Equal(userA.FormattedAddress(), resp.PendingTransfer.Sender)
		s.Require().Equal(transferAmount.String(), resp.PendingTransfer.Amount.String())
		s.T().Logf("Pending transfer exists: sender=%s, amount=%s", resp.PendingTransfer.Sender, resp.PendingTransfer.Amount)
	}))

	var prefetchedRelayTx []byte
	s.Require().True(s.Run("Prefetch relay tx before timeout", func() {
		sendTxHashBytes, err := hex.DecodeString(sendTxHash)
		s.Require().NoError(err)

		resp, err := s.RelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
			SrcChain:    s.ChainA.Config().ChainID,
			DstChain:    s.ChainB.Config().ChainID,
			SourceTxIds: [][]byte{sendTxHashBytes},
			SrcClientId: ibctesting.FirstClientID,
			DstClientId: ibctesting.FirstClientID,
		})
		s.Require().NoError(err)
		s.Require().NotEmpty(resp.Tx)
		prefetchedRelayTx = resp.Tx
		s.T().Log("Successfully prefetched relay tx before timeout")
	}))

	s.Require().True(s.Run("Wait for timeout to expire", func() {
		s.T().Log("Waiting 35 seconds for timeout to expire...")
		time.Sleep(35 * time.Second)
	}))

	s.Require().True(s.Run("Relay timeout packet back to Chain A", func() {
		sendTxHashBytes, err := hex.DecodeString(sendTxHash)
		s.Require().NoError(err)

		resp, err := s.RelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
			SrcChain:     s.ChainB.Config().ChainID,
			DstChain:     s.ChainA.Config().ChainID,
			TimeoutTxIds: [][]byte{sendTxHashBytes},
			SrcClientId:  ibctesting.FirstClientID,
			DstClientId:  ibctesting.FirstClientID,
		})
		s.Require().NoError(err)
		s.Require().NotEmpty(resp.Tx, "relayer should generate timeout tx")

		_ = s.MustBroadcastSdkTxBody(ctx, s.ChainA, s.ChainASubmitter, 2_000_000, resp.Tx)
	}))

	s.Require().True(s.Run("Verify tokens refunded on Chain A", func() {
		balance := s.queryBalance(ctx, s.ChainA, userA.FormattedAddress(), denomA)
		s.Require().True(balance.Equal(transferAmount), "expected %s (refunded), got %s", transferAmount, balance)
		s.T().Logf("User balance after timeout refund: %s", balance)
	}))

	s.Require().True(s.Run("Verify pending transfer removed after timeout", func() {
		_, err := s.queryPendingTransfer(ctx, s.ChainA, denomA, ibctesting.FirstClientID, 1)
		s.Require().Error(err, "pending transfer should be removed after timeout")
		s.T().Logf("Pending transfer removed as expected: %v", err)
	}))

	s.Require().True(s.Run("Verify no balance on Chain B", func() {
		balance := s.queryBalance(ctx, s.ChainB, userA.FormattedAddress(), denomB)
		s.Require().True(balance.IsZero(), "Chain B should have no tokens since transfer timed out, got %s", balance)
		s.T().Logf("Chain B balance is zero as expected")
	}))

	s.Require().True(s.Run("Constructing relay packet after timeout should fail", func() {
		sendTxHashBytes, err := hex.DecodeString(sendTxHash)
		s.Require().NoError(err)

		resp, err := s.RelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
			SrcChain:    s.ChainA.Config().ChainID,
			DstChain:    s.ChainB.Config().ChainID,
			SourceTxIds: [][]byte{sendTxHashBytes},
			SrcClientId: ibctesting.FirstClientID,
			DstClientId: ibctesting.FirstClientID,
		})
		s.Require().Error(err, "relayer should reject timed-out packet")
		s.Require().Nil(resp)
		s.T().Logf("Relayer correctly rejected timed-out packet: %v", err)
	}))

	s.Require().True(s.Run("Receiving packets on Chain B after timeout should fail", func() {
		resp, err := s.BroadcastSdkTxBody(ctx, s.ChainB, s.ChainBSubmitter, 2_000_000, prefetchedRelayTx)
		s.Require().Error(err, "chain should reject timed-out packet")
		s.Require().Nil(resp)
		s.T().Logf("Chain B correctly rejected timed-out packet: %v", err)
	}))
}
