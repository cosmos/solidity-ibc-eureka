package main

import (
	"context"
	"encoding/binary"
	"encoding/hex"
	"testing"
	"time"

	"github.com/cosmos/gogoproto/proto"
	"github.com/stretchr/testify/suite"

	solanago "github.com/gagliardetto/solana-go"
	"github.com/gagliardetto/solana-go/programs/token"
	"github.com/gagliardetto/solana-go/rpc"

	sdkmath "cosmossdk.io/math"

	sdk "github.com/cosmos/cosmos-sdk/types"
	authtypes "github.com/cosmos/cosmos-sdk/x/auth/types"
	govtypes "github.com/cosmos/cosmos-sdk/x/gov/types"

	gmptypes "github.com/cosmos/ibc-go/v10/modules/apps/27-gmp/types"

	interchaintest "github.com/cosmos/interchaintest/v10"
	"github.com/cosmos/interchaintest/v10/chain/cosmos"
	"github.com/cosmos/interchaintest/v10/ibc"

	access_manager "github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/accessmanager"
	ics26_router "github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/ics26router"
	ics27_gmp "github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/ics27gmp"
	ics27_ift "github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/ics27ift"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/chainconfig"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/e2esuite"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/solana"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
	relayertypes "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/relayer"
	solanatypes "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/solana"
	ifttypes "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/wfchain/ift"
	tokenfactorytypes "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/wfchain/tokenfactory"
)

// IbcEurekaSolanaIFTTestSuite tests ICS27-IFT functionality
type IbcEurekaSolanaIFTTestSuite struct {
	IbcEurekaSolanaTestSuite

	Wfchain         *cosmos.CosmosChain
	CosmosSubmitter ibc.Wallet

	IFTMint              solanago.PublicKey
	IFTMintAuthority     solanago.PublicKey
	IFTAppState          solanago.PublicKey
	IFTBridge            solanago.PublicKey
	SenderTokenAccount   solanago.PublicKey
	ReceiverTokenAccount solanago.PublicKey
}

const (
	IFTPortID         = testvalues.SolanaGMPPortID // IFT uses GMP port for transport
	IFTTokenDecimals  = uint8(6)
	IFTMintAmount     = uint64(10_000_000) // 10 tokens with 6 decimals
	IFTTransferAmount = uint64(1_000_000)  // 1 token with 6 decimals
)

var associatedTokenProgramID = solanago.MustPublicKeyFromBase58("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL")

func TestWithIbcEurekaSolanaIFTTestSuite(t *testing.T) {
	s := &IbcEurekaSolanaIFTTestSuite{}
	suite.Run(t, s)
}

// SetupSuite overrides the base suite to use wfchain (Cosmos with IFT module)
func (s *IbcEurekaSolanaIFTTestSuite) SetupSuite(ctx context.Context) {
	chainconfig.DefaultChainSpecs = []*interchaintest.ChainSpec{
		chainconfig.WfchainChainSpec("wfchain-1", "wfchain-1"),
	}

	s.UseMockWasmClient = true
	s.IbcEurekaSolanaTestSuite.SetupSuite(ctx)

	s.Wfchain = s.Cosmos.Chains[0]
	s.CosmosSubmitter = s.CreateAndFundCosmosUser(ctx, s.Wfchain)
}

// createTokenFactoryDenom creates a tokenfactory denom and returns the subdenom
func (s *IbcEurekaSolanaIFTTestSuite) createTokenFactoryDenom(ctx context.Context, subdenom string) string {
	msg := &tokenfactorytypes.MsgCreateDenom{
		Sender: s.CosmosSubmitter.FormattedAddress(),
		Denom:  subdenom,
	}
	_, err := s.BroadcastMessages(ctx, s.Wfchain, s.CosmosSubmitter, 200_000, msg)
	s.Require().NoError(err)
	return subdenom
}

func (s *IbcEurekaSolanaIFTTestSuite) registerCosmosIFTBridge(ctx context.Context, denom, clientId, counterpartyIftAddr, counterpartyClientId string, gmpProgramID, mint solanago.PublicKey) {
	govModuleAddr, err := s.Wfchain.AuthQueryModuleAddress(ctx, govtypes.ModuleName)
	s.Require().NoError(err)

	// counterpartyClientId is the client on Solana that tracks Cosmos - needed for gmp_account_pda derivation
	constructor := testvalues.BuildSolanaIFTConstructor(gmpProgramID.String(), mint.String(), counterpartyClientId)
	s.T().Logf("IFT constructor: %s", constructor)

	msg := &ifttypes.MsgRegisterIFTBridge{
		Signer:                 govModuleAddr,
		Denom:                  denom,
		ClientId:               clientId,
		CounterpartyIftAddress: counterpartyIftAddr,
		IftSendCallConstructor: constructor,
	}
	err = s.ExecuteGovV1Proposal(ctx, msg, s.Wfchain, s.CosmosSubmitter)
	s.Require().NoError(err)
}

func (s *IbcEurekaSolanaIFTTestSuite) getCosmosIFTModuleAddress() string {
	iftAddr := authtypes.NewModuleAddress(testvalues.IFTModuleName)
	bech32Addr, err := sdk.Bech32ifyAddressBytes(s.Wfchain.Config().Bech32Prefix, iftAddr)
	s.Require().NoError(err)
	return bech32Addr
}

// registerIFTBridge registers an IFT bridge for the Cosmos counterparty
func (s *IbcEurekaSolanaIFTTestSuite) registerIFTBridge(ctx context.Context, clientID, counterpartyAddress, counterpartyDenom string) {
	s.Require().True(s.Run("Register IFT Bridge", func() {
		bridgePDA, _ := solana.Ics27Ift.IftBridgePDA(ics27_ift.ProgramID, s.IFTMint[:], []byte(clientID))
		s.IFTBridge = bridgePDA

		accessManagerPDA, _ := solana.AccessManager.AccessManagerPDA(access_manager.ProgramID)

		// Query the ICA address on Cosmos for the IFT program
		// When IFT calls GMP via CPI, the sender is the IFT program ID (not the user)
		// This is similar to Ethereum where the IFT contract address is the sender
		iftProgramAddress := ics27_ift.ProgramID.String()
		res, err := e2esuite.GRPCQuery[gmptypes.QueryAccountAddressResponse](ctx, s.Wfchain, &gmptypes.QueryAccountAddressRequest{
			ClientId: CosmosClientID, // The wasm client on Cosmos (dest client)
			Sender:   iftProgramAddress,
			Salt:     "",
		})
		s.Require().NoError(err)
		s.Require().NotEmpty(res.AccountAddress)
		cosmosIcaAddress := res.AccountAddress
		s.T().Logf("Computed Cosmos ICA address: %s (for IFT program: %s)", cosmosIcaAddress, iftProgramAddress)

		registerMsg := ics27_ift.Ics27IftStateRegisterIftBridgeMsg{
			ClientId:               clientID,
			CounterpartyIftAddress: counterpartyAddress,
			CounterpartyDenom:      counterpartyDenom,
			CosmosTypeUrl:          "/wfchain.ift.MsgIFTMint", // Type URL for Cosmos MsgIFTMint
			CosmosIcaAddress:       cosmosIcaAddress,
			CounterpartyChainType:  ics27_ift.Ics27IftStateCounterpartyChainType_Cosmos,
		}

		registerIx, err := ics27_ift.NewRegisterIftBridgeInstruction(
			registerMsg,
			s.IFTAppState,
			bridgePDA,
			accessManagerPDA,
			s.SolanaRelayer.PublicKey(),       // Authority
			solanago.SysVarInstructionsPubkey, // Instructions sysvar
			s.SolanaRelayer.PublicKey(),       // Payer
			solanago.SystemProgramID,
		)
		s.Require().NoError(err)

		tx, err := s.Solana.Chain.NewTransactionFromInstructions(s.SolanaRelayer.PublicKey(), registerIx)
		s.Require().NoError(err)

		_, err = s.Solana.Chain.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentConfirmed, s.SolanaRelayer)
		s.Require().NoError(err)

		s.T().Logf("IFT Bridge registered for client %s", clientID)
		s.T().Logf("  Bridge PDA: %s", bridgePDA)
		s.T().Logf("  Counterparty: %s (Cosmos)", counterpartyAddress)
		s.T().Logf("  Counterparty Denom: %s", counterpartyDenom)
		s.T().Logf("  Cosmos ICA Address: %s", cosmosIcaAddress)
	}))
}

// initializeIFT initializes the IFT program for a given mint
func (s *IbcEurekaSolanaIFTTestSuite) initializeIFT(ctx context.Context, mint solanago.PublicKey) {
	appStatePDA, _ := solana.Ics27Ift.IftAppStatePDA(ics27_ift.ProgramID, mint[:])
	mintAuthorityPDA, _ := solana.Ics27Ift.IftMintAuthorityPDA(ics27_ift.ProgramID, mint[:])

	s.IFTAppState = appStatePDA
	s.IFTMintAuthority = mintAuthorityPDA

	initIx, err := ics27_ift.NewInitializeInstruction(
		IFTTokenDecimals,
		access_manager.ProgramID,
		ics27_gmp.ProgramID,
		appStatePDA,
		mint,
		mintAuthorityPDA,
		s.SolanaRelayer.PublicKey(),
		s.SolanaRelayer.PublicKey(),
		token.ProgramID,
		solanago.SystemProgramID,
	)
	s.Require().NoError(err)

	tx, err := s.Solana.Chain.NewTransactionFromInstructions(s.SolanaRelayer.PublicKey(), initIx)
	s.Require().NoError(err)

	_, err = s.Solana.Chain.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentConfirmed, s.SolanaRelayer)
	s.Require().NoError(err)
}

// Test_IFT_SolanaToCosmosTransfer tests the full roundtrip: Solana → wfchain → ack back
func (s *IbcEurekaSolanaIFTTestSuite) Test_IFT_SolanaToCosmosTransfer() {
	ctx := context.Background()
	s.SetupSuite(ctx)
	s.initializeICS27GMP(ctx)

	cosmosUser := s.Cosmos.Users[0]

	var cosmosDenom string
	s.Require().True(s.Run("Create tokenfactory denom", func() {
		cosmosDenom = s.createTokenFactoryDenom(ctx, testvalues.IFTTestDenom)
	}))

	var senderTokenAccount solanago.PublicKey
	s.Require().True(s.Run("Create SPL Token and Mint Initial Tokens", func() {
		mint, err := s.Solana.Chain.CreateSPLTokenMint(ctx, s.SolanaRelayer, IFTTokenDecimals)
		s.Require().NoError(err)
		s.IFTMint = mint

		tokenAccount, err := s.Solana.Chain.CreateOrGetAssociatedTokenAccount(ctx, s.SolanaRelayer, mint, s.SolanaRelayer.PublicKey())
		s.Require().NoError(err)
		senderTokenAccount = tokenAccount
		s.SenderTokenAccount = tokenAccount

		err = s.Solana.Chain.MintTokensTo(ctx, s.SolanaRelayer, mint, tokenAccount, IFTMintAmount)
		s.Require().NoError(err)

		balance, err := s.Solana.Chain.GetTokenBalance(ctx, tokenAccount)
		s.Require().NoError(err)
		s.Require().Equal(IFTMintAmount, balance)
	}))

	s.initializeIFT(ctx, s.IFTMint)

	s.Require().True(s.Run("Register Cosmos IFT Bridge", func() {
		// SolanaClientID is the client on Solana tracking Cosmos - needed for gmp_account_pda derivation
		s.registerCosmosIFTBridge(ctx, cosmosDenom, testvalues.FirstWasmClientID, ics27_ift.ProgramID.String(), SolanaClientID, ics27_gmp.ProgramID, s.IFTMint)
	}))

	iftModuleAddr := s.getCosmosIFTModuleAddress()
	s.T().Logf("DEBUG: iftModuleAddr (registered on Solana) = %s", iftModuleAddr)
	s.T().Logf("DEBUG: Compare with GMP packet sender in relayer logs above")
	s.registerIFTBridge(ctx, SolanaClientID, iftModuleAddr, cosmosDenom)

	initialBalance, err := s.Solana.Chain.GetTokenBalance(ctx, senderTokenAccount)
	s.Require().NoError(err)

	var transferTxSig solanago.Signature
	var baseSequence uint64
	s.Require().True(s.Run("Execute IFT Transfer", func() {
		gmpAppStatePDA, _ := solana.Ics27Gmp.AppStateGmpportPDA(ics27_gmp.ProgramID)
		routerStatePDA, _ := solana.Ics26Router.RouterStatePDA(ics26_router.ProgramID)
		ibcClientPDA, _ := solana.Ics26Router.ClientWithArgSeedPDA(ics26_router.ProgramID, []byte(SolanaClientID))
		gmpIbcAppPDA, _ := solana.Ics26Router.IbcAppWithArgSeedPDA(ics26_router.ProgramID, []byte(GMPPortID))
		clientSequencePDA, _ := solana.Ics26Router.ClientSequenceWithArgSeedPDA(ics26_router.ProgramID, []byte(SolanaClientID))

		baseSequence, err = s.Solana.Chain.GetNextSequenceNumber(ctx, clientSequencePDA)
		s.Require().NoError(err)

		namespacedSequence := solana.CalculateNamespacedSequence(baseSequence, ics27_gmp.ProgramID, s.SolanaRelayer.PublicKey())
		namespacedSequenceBytes := make([]byte, 8)
		binary.LittleEndian.PutUint64(namespacedSequenceBytes, namespacedSequence)

		packetCommitmentPDA, _ := solana.Ics26Router.PacketCommitmentWithArgSeedPDA(ics26_router.ProgramID, []byte(SolanaClientID), namespacedSequenceBytes)
		pendingTransferPDA, _ := solana.Ics27Ift.PendingTransferPDA(ics27_ift.ProgramID, s.IFTMint[:], []byte(SolanaClientID), namespacedSequenceBytes)

		solanaClockTime, err := s.Solana.Chain.GetSolanaClockTime(ctx)
		s.Require().NoError(err)
		timeoutTimestamp := solanaClockTime + 900

		transferMsg := ics27_ift.Ics27IftStateIftTransferMsg{
			ClientId:         SolanaClientID,
			Receiver:         cosmosUser.FormattedAddress(),
			Amount:           IFTTransferAmount,
			TimeoutTimestamp: timeoutTimestamp,
		}

		transferIx, err := ics27_ift.NewIftTransferInstruction(
			transferMsg, s.IFTAppState, s.IFTBridge, s.IFTMint, senderTokenAccount,
			s.SolanaRelayer.PublicKey(), s.SolanaRelayer.PublicKey(),
			token.ProgramID, solanago.SystemProgramID, ics27_gmp.ProgramID, gmpAppStatePDA,
			ics26_router.ProgramID, routerStatePDA, clientSequencePDA, packetCommitmentPDA,
			solanago.SysVarInstructionsPubkey, gmpIbcAppPDA, ibcClientPDA, pendingTransferPDA,
		)
		s.Require().NoError(err)

		computeBudgetIx := solana.NewComputeBudgetInstruction(400_000)
		tx, err := s.Solana.Chain.NewTransactionFromInstructions(s.SolanaRelayer.PublicKey(), computeBudgetIx, transferIx)
		s.Require().NoError(err)

		transferTxSig, err = s.Solana.Chain.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentConfirmed, s.SolanaRelayer)
		s.Require().NoError(err)
		s.T().Logf("IFT transfer tx: %s", transferTxSig)
	}))

	s.Require().True(s.Run("Verify Token Burn on Solana", func() {
		burnedBalance, err := s.Solana.Chain.GetTokenBalance(ctx, senderTokenAccount)
		s.Require().NoError(err)
		s.Require().Equal(initialBalance-IFTTransferAmount, burnedBalance)
	}))

	var cosmosRecvTxHash string
	s.Require().True(s.Run("Relay packet to wfchain", func() {
		resp, err := s.RelayerClient.RelayByTx(ctx, &relayertypes.RelayByTxRequest{
			SrcChain:    testvalues.SolanaChainID,
			DstChain:    s.Wfchain.Config().ChainID,
			SourceTxIds: [][]byte{[]byte(transferTxSig.String())},
			SrcClientId: SolanaClientID,
			DstClientId: CosmosClientID,
		})
		s.Require().NoError(err)
		s.Require().NotEmpty(resp.Tx)

		receipt := s.MustBroadcastSdkTxBody(ctx, s.Wfchain, s.Cosmos.Users[0], 2_000_000, resp.Tx)
		cosmosRecvTxHash = receipt.TxHash
		s.T().Logf("Packet relayed to wfchain: %s", cosmosRecvTxHash)
	}))

	s.Require().True(s.Run("Verify tokens minted on wfchain", func() {
		balance, err := s.Wfchain.GetBalance(ctx, cosmosUser.FormattedAddress(), cosmosDenom)
		s.Require().NoError(err)
		expectedAmount := sdkmath.NewInt(int64(IFTTransferAmount))
		s.Require().True(balance.Equal(expectedAmount), "expected %s, got %s", expectedAmount, balance)
		s.T().Logf("wfchain balance: %s", balance)
	}))

	s.Require().True(s.Run("Verify PendingTransfer PDA exists before ack", func() {
		namespacedSequence := solana.CalculateNamespacedSequence(baseSequence, ics27_gmp.ProgramID, s.SolanaRelayer.PublicKey())
		s.Solana.Chain.VerifyPendingTransferExists(ctx, s.T(), s.Require(), ics27_ift.ProgramID, s.IFTMint, SolanaClientID, namespacedSequence)
	}))

	s.Require().True(s.Run("Relay ack back to Solana", func() {
		cosmosRecvTxHashBytes, err := hex.DecodeString(cosmosRecvTxHash)
		s.Require().NoError(err)

		resp, err := s.RelayerClient.RelayByTx(ctx, &relayertypes.RelayByTxRequest{
			SrcChain:    s.Wfchain.Config().ChainID,
			DstChain:    testvalues.SolanaChainID,
			SourceTxIds: [][]byte{cosmosRecvTxHashBytes},
			SrcClientId: CosmosClientID,
			DstClientId: SolanaClientID,
		})
		s.Require().NoError(err)
		s.Require().NotEmpty(resp.Tx)

		sig, err := s.Solana.Chain.SubmitChunkedRelayPackets(ctx, s.T(), resp, s.SolanaRelayer)
		s.Require().NoError(err)
		s.T().Logf("Ack relayed to Solana: %s", sig)
	}))

	s.Require().True(s.Run("Verify PendingTransfer PDA closed", func() {
		namespacedSequence := solana.CalculateNamespacedSequence(baseSequence, ics27_gmp.ProgramID, s.SolanaRelayer.PublicKey())
		s.Solana.Chain.VerifyPendingTransferClosed(ctx, s.T(), s.Require(), ics27_ift.ProgramID, s.IFTMint, SolanaClientID, namespacedSequence)
	}))

	s.Require().True(s.Run("Verify final Solana balance unchanged (success path)", func() {
		finalBalance, err := s.Solana.Chain.GetTokenBalance(ctx, senderTokenAccount)
		s.Require().NoError(err)
		s.Require().Equal(initialBalance-IFTTransferAmount, finalBalance, "Balance should remain burned after success ack")
	}))

	// === Roundtrip: wfchain → Solana ===
	var cosmosIFTTxHash string
	s.Require().True(s.Run("Transfer back: wfchain → Solana", func() {
		s.Require().True(s.Run("Execute IFT transfer from wfchain", func() {
			timeout := uint64(time.Now().Add(15 * time.Minute).Unix())
			receiverPubkey := s.SolanaRelayer.PublicKey()

			resp, err := s.BroadcastMessages(ctx, s.Wfchain, cosmosUser, 200_000, &ifttypes.MsgIFTTransfer{
				Signer:           cosmosUser.FormattedAddress(),
				Denom:            cosmosDenom,
				ClientId:         testvalues.FirstWasmClientID,
				Receiver:         receiverPubkey.String(),
				Amount:           sdkmath.NewInt(int64(IFTTransferAmount)),
				TimeoutTimestamp: timeout,
			})
			s.Require().NoError(err)
			cosmosIFTTxHash = resp.TxHash
			s.T().Logf("wfchain IFT transfer: %s", cosmosIFTTxHash)
		}))

		s.Require().True(s.Run("Verify wfchain balance burned", func() {
			balance, err := s.Wfchain.GetBalance(ctx, cosmosUser.FormattedAddress(), cosmosDenom)
			s.Require().NoError(err)
			s.Require().True(balance.IsZero(), "expected 0 after burn, got %s", balance)
		}))

		var solanaRelayTxSig solanago.Signature
		s.Require().True(s.Run("Relay to Solana and execute IFT mint", func() {
			cosmosIFTTxHashBytes, err := hex.DecodeString(cosmosIFTTxHash)
			s.Require().NoError(err)

			resp, err := s.RelayerClient.RelayByTx(ctx, &relayertypes.RelayByTxRequest{
				SrcChain:    s.Wfchain.Config().ChainID,
				DstChain:    testvalues.SolanaChainID,
				SourceTxIds: [][]byte{cosmosIFTTxHashBytes},
				SrcClientId: CosmosClientID,
				DstClientId: SolanaClientID,
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Tx)

			solanaRelayTxSig, err = s.Solana.Chain.SubmitChunkedRelayPackets(ctx, s.T(), resp, s.SolanaRelayer)
			s.Require().NoError(err)
			s.T().Logf("Solana relay tx: %s", solanaRelayTxSig)
		}))

		s.Require().True(s.Run("Verify tokens minted on Solana", func() {
			balance, err := s.Solana.Chain.GetTokenBalance(ctx, senderTokenAccount)
			s.Require().NoError(err)
			s.Require().Equal(initialBalance, balance, "Balance should be restored after roundtrip")
			s.T().Logf("Solana balance after roundtrip: %d", balance)
		}))

		s.Require().True(s.Run("Relay ack to wfchain", func() {
			resp, err := s.RelayerClient.RelayByTx(ctx, &relayertypes.RelayByTxRequest{
				SrcChain:    testvalues.SolanaChainID,
				DstChain:    s.Wfchain.Config().ChainID,
				SourceTxIds: [][]byte{[]byte(solanaRelayTxSig.String())},
				SrcClientId: SolanaClientID,
				DstClientId: CosmosClientID,
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Tx)

			_ = s.MustBroadcastSdkTxBody(ctx, s.Wfchain, s.Cosmos.Users[0], 2_000_000, resp.Tx)
		}))
	}))
}

// Test_IFT_CosmosToSolanaTransfer tests receiving IFT tokens from wfchain to Solana
func (s *IbcEurekaSolanaIFTTestSuite) Test_IFT_CosmosToSolanaTransfer() {
	ctx := context.Background()
	s.SetupSuite(ctx)
	s.initializeICS27GMP(ctx)

	s.Require().True(s.Run("Create SPL Token", func() {
		mint, err := s.Solana.Chain.CreateSPLTokenMint(ctx, s.SolanaRelayer, IFTTokenDecimals)
		s.Require().NoError(err)
		s.IFTMint = mint
	}))

	s.initializeIFT(ctx, s.IFTMint)

	cosmosUser := s.Cosmos.Users[0]
	s.registerIFTBridge(ctx, SolanaClientID, cosmosUser.FormattedAddress(), testvalues.IFTTestDenom)

	var receiverTokenAccount solanago.PublicKey
	receiverPubkey := s.SolanaRelayer.PublicKey()
	s.Require().True(s.Run("Derive Receiver ATA Address", func() {
		ataAddr, err := solana.AssociatedTokenAccountAddress(receiverPubkey, s.IFTMint)
		s.Require().NoError(err)
		receiverTokenAccount = ataAddr
		s.ReceiverTokenAccount = ataAddr
	}))

	var initialBalance uint64
	s.Require().True(s.Run("Get Initial Balance", func() {
		balance, err := s.Solana.Chain.GetTokenBalance(ctx, receiverTokenAccount)
		if err != nil {
			initialBalance = 0
		} else {
			initialBalance = balance
		}
	}))

	var cosmosGMPTxHash []byte
	s.Require().True(s.Run("Send GMP Call from Cosmos", func() {
		gmpAccountPDA, gmpAccountBump := solana.Ics27Gmp.GmpAccountPDA(
			ics27_gmp.ProgramID,
			[]byte(SolanaClientID),
			[]byte(cosmosUser.FormattedAddress()),
			[]byte{},
		)

		iftMintMsg := ics27_ift.Ics27IftStateIftMintMsg{
			Receiver:       receiverPubkey,
			Amount:         IFTTransferAmount,
			ClientId:       SolanaClientID,
			GmpAccountBump: gmpAccountBump,
		}

		msgBytes, err := iftMintMsg.Marshal()
		s.Require().NoError(err)

		iftMintData := make([]byte, 0, len(ics27_ift.Instruction_IftMint)+len(msgBytes))
		iftMintData = append(iftMintData, ics27_ift.Instruction_IftMint[:]...)
		iftMintData = append(iftMintData, msgBytes...)

		payerPosition := uint32(8)

		accounts := []*solanatypes.SolanaAccountMeta{
			{Pubkey: s.IFTAppState[:], IsWritable: true, IsSigner: false},
			{Pubkey: s.IFTBridge[:], IsWritable: false, IsSigner: false},
			{Pubkey: s.IFTMint[:], IsWritable: true, IsSigner: false},
			{Pubkey: s.IFTMintAuthority[:], IsWritable: false, IsSigner: false},
			{Pubkey: receiverTokenAccount[:], IsWritable: true, IsSigner: false},
			{Pubkey: receiverPubkey[:], IsWritable: false, IsSigner: false},
			{Pubkey: ics27_gmp.ProgramID[:], IsWritable: false, IsSigner: false},
			{Pubkey: gmpAccountPDA[:], IsWritable: false, IsSigner: true},
			{Pubkey: token.ProgramID[:], IsWritable: false, IsSigner: false},
			{Pubkey: associatedTokenProgramID[:], IsWritable: false, IsSigner: false},
			{Pubkey: solanago.SystemProgramID[:], IsWritable: false, IsSigner: false},
		}

		gmpPayload := &solanatypes.GMPSolanaPayload{
			Data:          iftMintData,
			Accounts:      accounts,
			PayerPosition: &payerPosition,
		}

		payloadBytes, err := proto.Marshal(gmpPayload)
		s.Require().NoError(err)

		timeout := uint64(time.Now().Add(15 * time.Minute).Unix())

		resp, err := s.BroadcastMessages(ctx, s.Wfchain, cosmosUser, 2_000_000, &gmptypes.MsgSendCall{
			SourceClient:     CosmosClientID,
			Sender:           cosmosUser.FormattedAddress(),
			Receiver:         ics27_ift.ProgramID.String(),
			Salt:             []byte{},
			Payload:          payloadBytes,
			TimeoutTimestamp: timeout,
			Memo:             "IFT mint via GMP",
			Encoding:         testvalues.Ics27ProtobufEncoding,
		})
		s.Require().NoError(err)

		cosmosGMPTxHashBytes, err := hex.DecodeString(resp.TxHash)
		s.Require().NoError(err)
		cosmosGMPTxHash = cosmosGMPTxHashBytes
	}))

	var solanaRelayTxSig solanago.Signature
	s.Require().True(s.Run("Relay and Execute IFT Mint on Solana", func() {
		resp, err := s.RelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
			SrcChain:    s.Wfchain.Config().ChainID,
			DstChain:    testvalues.SolanaChainID,
			SourceTxIds: [][]byte{cosmosGMPTxHash},
			SrcClientId: CosmosClientID,
			DstClientId: SolanaClientID,
		})
		s.Require().NoError(err)
		s.Require().NotEmpty(resp.Tx, "Relay should return transaction")

		solanaRelayTxSig, err = s.Solana.Chain.SubmitChunkedRelayPackets(ctx, s.T(), resp, s.SolanaRelayer)
		s.Require().NoError(err)
	}))

	s.Require().True(s.Run("Verify Token Mint", func() {
		finalBalance, err := s.Solana.Chain.GetTokenBalance(ctx, receiverTokenAccount)
		s.Require().NoError(err)
		expectedBalance := initialBalance + IFTTransferAmount
		s.Require().Equal(expectedBalance, finalBalance, "Tokens should be minted to receiver")
	}))

	s.Require().True(s.Run("Relay Acknowledgment to Cosmos", func() {
		var ackRelayTxBodyBz []byte
		s.Require().True(s.Run("Retrieve acknowledgment relay tx", func() {
			resp, err := s.RelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
				SrcChain:    testvalues.SolanaChainID,
				DstChain:    s.Wfchain.Config().ChainID,
				SourceTxIds: [][]byte{[]byte(solanaRelayTxSig.String())},
				SrcClientId: SolanaClientID,
				DstClientId: CosmosClientID,
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Tx)
			ackRelayTxBodyBz = resp.Tx
		}))

		s.Require().True(s.Run("Broadcast acknowledgment on Cosmos", func() {
			_ = s.MustBroadcastSdkTxBody(ctx, s.Wfchain, cosmosUser, CosmosDefaultGasLimit, ackRelayTxBodyBz)
		}))
	}))
}

// Test_IFT_AdminSetupFlow tests the SPL token setup and IFT initialization flow
func (s *IbcEurekaSolanaIFTTestSuite) Test_IFT_AdminSetupFlow() {
	ctx := context.Background()
	s.SetupSuite(ctx)
	s.initializeICS27GMP(ctx)

	var mint solanago.PublicKey
	var initialMintAuthority solanago.PublicKey
	var adminTokenAccount solanago.PublicKey

	s.Require().True(s.Run("Create SPL Token Mint (admin is mint authority)", func() {
		var err error
		mint, err = s.Solana.Chain.CreateSPLTokenMint(ctx, s.SolanaRelayer, IFTTokenDecimals)
		s.Require().NoError(err)
		s.IFTMint = mint

		// The mint authority should be the admin (SolanaRelayer) initially
		initialMintAuthority = s.SolanaRelayer.PublicKey()
		s.Solana.Chain.VerifyMintAuthority(ctx, s.T(), s.Require(), mint, initialMintAuthority)
		s.T().Logf("SPL Token mint created: %s", mint.String())
		s.T().Logf("Initial mint authority (admin): %s", initialMintAuthority.String())
	}))

	s.Require().True(s.Run("Verify admin can mint tokens before IFT initialization", func() {
		var err error
		adminTokenAccount, err = s.Solana.Chain.CreateOrGetAssociatedTokenAccount(ctx, s.SolanaRelayer, mint, s.SolanaRelayer.PublicKey())
		s.Require().NoError(err)

		// Admin should be able to mint tokens
		err = s.Solana.Chain.MintTokensTo(ctx, s.SolanaRelayer, mint, adminTokenAccount, IFTMintAmount)
		s.Require().NoError(err)

		balance, err := s.Solana.Chain.GetTokenBalance(ctx, adminTokenAccount)
		s.Require().NoError(err)
		s.Require().Equal(IFTMintAmount, balance, "Admin should have minted tokens")
		s.T().Logf("Admin successfully minted %d tokens", IFTMintAmount)
	}))

	var expectedMintAuthority solanago.PublicKey
	s.Require().True(s.Run("Initialize IFT (transfer mint authority to IFT PDA)", func() {
		s.initializeIFT(ctx, mint)

		// After initialization, mint authority should be the IFT mint authority PDA
		expectedMintAuthority, _ = solana.Ics27Ift.IftMintAuthorityPDA(ics27_ift.ProgramID, mint[:])
		s.Solana.Chain.VerifyMintAuthority(ctx, s.T(), s.Require(), mint, expectedMintAuthority)
		s.T().Logf("IFT initialized - mint authority transferred to: %s", expectedMintAuthority.String())
	}))

	var bridgePDA solanago.PublicKey
	cosmosCounterpartyAddress := "cosmos1test123456789" // Mock counterparty
	s.Require().True(s.Run("Register IFT Bridge", func() {
		s.registerIFTBridge(ctx, SolanaClientID, cosmosCounterpartyAddress, testvalues.IFTTestDenom)
		bridgePDA = s.IFTBridge
		s.T().Logf("IFT Bridge registered: %s", bridgePDA.String())
	}))

	s.Require().True(s.Run("Verify bridge is active", func() {
		// Fetch bridge account and verify it's active
		accountInfo, err := s.Solana.Chain.RPCClient.GetAccountInfoWithOpts(ctx, bridgePDA, &rpc.GetAccountInfoOpts{
			Commitment: rpc.CommitmentConfirmed,
		})
		s.Require().NoError(err)
		s.Require().NotNil(accountInfo.Value, "Bridge account should exist")
		s.Require().True(accountInfo.Value.Lamports > 0, "Bridge account should have lamports")
		s.T().Logf("Bridge account verified: %d lamports", accountInfo.Value.Lamports)
	}))
}

// Test_IFT_RevokeMintAuthority tests that admin can revoke mint authority from IFT
func (s *IbcEurekaSolanaIFTTestSuite) Test_IFT_RevokeMintAuthority() {
	ctx := context.Background()
	s.SetupSuite(ctx)
	s.initializeICS27GMP(ctx)

	var mint solanago.PublicKey
	var initialMintAuthority solanago.PublicKey

	s.Require().True(s.Run("Create SPL Token Mint", func() {
		var err error
		mint, err = s.Solana.Chain.CreateSPLTokenMint(ctx, s.SolanaRelayer, IFTTokenDecimals)
		s.Require().NoError(err)
		s.IFTMint = mint

		initialMintAuthority = s.SolanaRelayer.PublicKey()
		s.Solana.Chain.VerifyMintAuthority(ctx, s.T(), s.Require(), mint, initialMintAuthority)
	}))

	var iftMintAuthorityPDA solanago.PublicKey
	s.Require().True(s.Run("Initialize IFT", func() {
		s.initializeIFT(ctx, mint)

		iftMintAuthorityPDA, _ = solana.Ics27Ift.IftMintAuthorityPDA(ics27_ift.ProgramID, mint[:])
		s.Solana.Chain.VerifyMintAuthority(ctx, s.T(), s.Require(), mint, iftMintAuthorityPDA)
		s.T().Logf("IFT initialized - mint authority: %s", iftMintAuthorityPDA)
	}))

	// Create new wallet to receive mint authority
	newAuthorityWallet, err := s.Solana.Chain.CreateAndFundWallet()
	s.Require().NoError(err)

	s.Require().True(s.Run("Verify app state exists before revoke", func() {
		s.Solana.Chain.VerifyIftAppStateExists(ctx, s.T(), s.Require(), ics27_ift.ProgramID, mint)
	}))

	s.Require().True(s.Run("Revoke mint authority", func() {
		accessManagerPDA, _ := solana.AccessManager.AccessManagerPDA(access_manager.ProgramID)

		revokeIx, err := ics27_ift.NewRevokeMintAuthorityInstruction(
			s.IFTAppState,
			mint,
			iftMintAuthorityPDA,
			newAuthorityWallet.PublicKey(),
			accessManagerPDA,
			s.SolanaRelayer.PublicKey(), // admin
			s.SolanaRelayer.PublicKey(), // payer
			solanago.SysVarInstructionsPubkey,
			token.ProgramID,
		)
		s.Require().NoError(err)

		tx, err := s.Solana.Chain.NewTransactionFromInstructions(s.SolanaRelayer.PublicKey(), revokeIx)
		s.Require().NoError(err)

		sig, err := s.Solana.Chain.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentConfirmed, s.SolanaRelayer)
		s.Require().NoError(err)
		s.T().Logf("Revoke mint authority tx: %s", sig)
	}))

	s.Require().True(s.Run("Verify mint authority transferred", func() {
		s.Solana.Chain.VerifyMintAuthority(ctx, s.T(), s.Require(), mint, newAuthorityWallet.PublicKey())
		s.T().Logf("✓ Mint authority transferred to: %s", newAuthorityWallet.PublicKey())
	}))

	s.Require().True(s.Run("Verify IFT app state closed", func() {
		s.Solana.Chain.VerifyIftAppStateClosed(ctx, s.T(), s.Require(), ics27_ift.ProgramID, mint)
	}))

	s.Require().True(s.Run("Verify new authority can mint tokens", func() {
		tokenAccount, err := s.Solana.Chain.CreateOrGetAssociatedTokenAccount(ctx, newAuthorityWallet, mint, newAuthorityWallet.PublicKey())
		s.Require().NoError(err)

		err = s.Solana.Chain.MintTokensTo(ctx, newAuthorityWallet, mint, tokenAccount, IFTMintAmount)
		s.Require().NoError(err)

		balance, err := s.Solana.Chain.GetTokenBalance(ctx, tokenAccount)
		s.Require().NoError(err)
		s.Require().Equal(IFTMintAmount, balance)
		s.T().Logf("✓ New authority minted %d tokens", IFTMintAmount)
	}))
}

// Test_IFT_TimeoutRefund tests that tokens are refunded on timeout
func (s *IbcEurekaSolanaIFTTestSuite) Test_IFT_TimeoutRefund() {
	ctx := context.Background()
	s.SetupSuite(ctx)
	s.initializeICS27GMP(ctx)

	var senderTokenAccount solanago.PublicKey
	s.Require().True(s.Run("Setup IFT with Initial Tokens", func() {
		mint, err := s.Solana.Chain.CreateSPLTokenMint(ctx, s.SolanaRelayer, IFTTokenDecimals)
		s.Require().NoError(err)
		s.IFTMint = mint

		tokenAccount, err := s.Solana.Chain.CreateOrGetAssociatedTokenAccount(ctx, s.SolanaRelayer, mint, s.SolanaRelayer.PublicKey())
		s.Require().NoError(err)
		senderTokenAccount = tokenAccount
		s.SenderTokenAccount = tokenAccount

		err = s.Solana.Chain.MintTokensTo(ctx, s.SolanaRelayer, mint, tokenAccount, IFTMintAmount)
		s.Require().NoError(err)
	}))

	s.initializeIFT(ctx, s.IFTMint)
	cosmosUser := s.Cosmos.Users[0]
	s.registerIFTBridge(ctx, SolanaClientID, cosmosUser.FormattedAddress(), testvalues.IFTTestDenom)

	initialBalance, err := s.Solana.Chain.GetTokenBalance(ctx, senderTokenAccount)
	s.Require().NoError(err)

	var solanaPacketTxHash []byte
	var baseSequence uint64
	s.Require().True(s.Run("Execute Transfer with Short Timeout", func() {
		routerStatePDA, _ := solana.Ics26Router.RouterStatePDA(ics26_router.ProgramID)
		clientSequencePDA, _ := solana.Ics26Router.ClientSequenceWithArgSeedPDA(ics26_router.ProgramID, []byte(SolanaClientID))
		gmpAppStatePDA, _ := solana.Ics27Gmp.AppStateGmpportPDA(ics27_gmp.ProgramID)
		gmpIbcAppPDA, _ := solana.Ics26Router.IbcAppWithArgSeedPDA(ics26_router.ProgramID, []byte(IFTPortID))
		ibcClientPDA, _ := solana.Ics26Router.ClientWithArgSeedPDA(ics26_router.ProgramID, []byte(SolanaClientID))

		var err error
		baseSequence, err = s.Solana.Chain.GetNextSequenceNumber(ctx, clientSequencePDA)
		s.Require().NoError(err)

		namespacedSequence := solana.CalculateNamespacedSequence(
			baseSequence,
			ics27_gmp.ProgramID,
			s.SolanaRelayer.PublicKey(),
		)

		seqBytes := make([]byte, 8)
		binary.LittleEndian.PutUint64(seqBytes, namespacedSequence)
		packetCommitmentPDA, _ := solana.Ics26Router.PacketCommitmentWithArgSeedPDA(ics26_router.ProgramID, []byte(SolanaClientID), seqBytes)
		pendingTransferPDA, _ := solana.Ics27Ift.PendingTransferPDA(ics27_ift.ProgramID, s.IFTMint[:], []byte(SolanaClientID), seqBytes)

		solanaClockTime, err := s.Solana.Chain.GetSolanaClockTime(ctx)
		s.Require().NoError(err)

		// Use 35 second timeout for faster test execution
		timeoutTimestamp := solanaClockTime + 35
		s.T().Logf("Setting timeout to: %d (solana_clock=%d + 35 seconds)", timeoutTimestamp, solanaClockTime)

		transferMsg := ics27_ift.Ics27IftStateIftTransferMsg{
			ClientId:         SolanaClientID,
			Receiver:         cosmosUser.FormattedAddress(),
			Amount:           IFTTransferAmount,
			TimeoutTimestamp: timeoutTimestamp,
		}

		transferIx, err := ics27_ift.NewIftTransferInstruction(
			transferMsg,
			s.IFTAppState,
			s.IFTBridge,
			s.IFTMint,
			senderTokenAccount,
			s.SolanaRelayer.PublicKey(),
			s.SolanaRelayer.PublicKey(),
			token.ProgramID,
			solanago.SystemProgramID,
			ics27_gmp.ProgramID,
			gmpAppStatePDA,
			ics26_router.ProgramID,
			routerStatePDA,
			clientSequencePDA,
			packetCommitmentPDA,
			solanago.SysVarInstructionsPubkey,
			gmpIbcAppPDA,
			ibcClientPDA,
			pendingTransferPDA,
		)
		s.Require().NoError(err)

		computeBudgetIx := solana.NewComputeBudgetInstruction(400_000)
		tx, err := s.Solana.Chain.NewTransactionFromInstructions(s.SolanaRelayer.PublicKey(), computeBudgetIx, transferIx)
		s.Require().NoError(err)

		sig, err := s.Solana.Chain.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentConfirmed, s.SolanaRelayer)
		s.Require().NoError(err)

		solanaPacketTxHash = []byte(sig.String())
		s.T().Logf("IFT transfer transaction (will timeout): %s", sig)
	}))

	burnedBalance, err := s.Solana.Chain.GetTokenBalance(ctx, senderTokenAccount)
	s.Require().NoError(err)
	s.Require().Equal(initialBalance-IFTTransferAmount, burnedBalance, "Tokens should be burned after transfer")

	s.Require().True(s.Run("Verify PendingTransfer PDA exists before timeout", func() {
		namespacedSequence := solana.CalculateNamespacedSequence(baseSequence, ics27_gmp.ProgramID, s.SolanaRelayer.PublicKey())
		s.Solana.Chain.VerifyPendingTransferExists(ctx, s.T(), s.Require(), ics27_ift.ProgramID, s.IFTMint, SolanaClientID, namespacedSequence)
	}))

	// Sleep for 40 seconds to let the packet timeout (timeout is set to solana_time + 35 seconds)
	s.T().Log("Sleeping 40 seconds to let packet timeout...")
	time.Sleep(40 * time.Second)

	s.Require().True(s.Run("Relay timeout back to Solana", func() {
		resp, err := s.RelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
			SrcChain:     s.Wfchain.Config().ChainID,
			DstChain:     testvalues.SolanaChainID,
			TimeoutTxIds: [][]byte{solanaPacketTxHash},
			SrcClientId:  CosmosClientID,
			DstClientId:  SolanaClientID,
		})
		s.Require().NoError(err)
		s.Require().NotEmpty(resp.Tx, "Relay should return transaction")

		sig, err := s.Solana.Chain.SubmitChunkedRelayPackets(ctx, s.T(), resp, s.SolanaRelayer)
		s.Require().NoError(err)
		s.T().Logf("Timeout transaction: %s", sig)
	}))

	s.Require().True(s.Run("Verify timeout effects", func() {
		s.Require().True(s.Run("Verify packet commitment deleted", func() {
			s.Solana.Chain.VerifyPacketCommitmentDeleted(ctx, s.T(), s.Require(), SolanaClientID, baseSequence, ics27_gmp.ProgramID, s.SolanaRelayer.PublicKey())
			s.T().Logf("Packet commitment successfully deleted for base sequence %d", baseSequence)
		}))

		s.Require().True(s.Run("Verify tokens refunded to sender", func() {
			refundedBalance, err := s.Solana.Chain.GetTokenBalance(ctx, senderTokenAccount)
			s.Require().NoError(err)
			s.Require().Equal(initialBalance, refundedBalance, "Tokens should be refunded to sender after timeout")
			s.T().Logf("Token balance after refund: %d (initial: %d)", refundedBalance, initialBalance)
		}))

		s.Require().True(s.Run("Verify PendingTransfer PDA closed", func() {
			namespacedSequence := solana.CalculateNamespacedSequence(
				baseSequence,
				ics27_gmp.ProgramID,
				s.SolanaRelayer.PublicKey(),
			)
			s.Solana.Chain.VerifyPendingTransferClosed(ctx, s.T(), s.Require(),
				ics27_ift.ProgramID, s.IFTMint, SolanaClientID, namespacedSequence)
		}))
	}))
}

// Test_IFT_AckFailureRefund tests that tokens are refunded on acknowledgement failure
// Note: wfchain has IFT module but we intentionally don't register the bridge to trigger error ack
func (s *IbcEurekaSolanaIFTTestSuite) Test_IFT_AckFailureRefund() {
	ctx := context.Background()
	s.SetupSuite(ctx)
	s.initializeICS27GMP(ctx)

	var senderTokenAccount solanago.PublicKey
	s.Require().True(s.Run("Setup IFT with Initial Tokens", func() {
		mint, err := s.Solana.Chain.CreateSPLTokenMint(ctx, s.SolanaRelayer, IFTTokenDecimals)
		s.Require().NoError(err)
		s.IFTMint = mint

		tokenAccount, err := s.Solana.Chain.CreateOrGetAssociatedTokenAccount(ctx, s.SolanaRelayer, mint, s.SolanaRelayer.PublicKey())
		s.Require().NoError(err)
		senderTokenAccount = tokenAccount
		s.SenderTokenAccount = tokenAccount

		err = s.Solana.Chain.MintTokensTo(ctx, s.SolanaRelayer, mint, tokenAccount, IFTMintAmount)
		s.Require().NoError(err)
	}))

	s.initializeIFT(ctx, s.IFTMint)

	cosmosUser := s.Cosmos.Users[0]
	s.registerIFTBridge(ctx, SolanaClientID, cosmosUser.FormattedAddress(), testvalues.IFTTestDenom)

	initialBalance, err := s.Solana.Chain.GetTokenBalance(ctx, senderTokenAccount)
	s.Require().NoError(err)

	var transferTxSig solanago.Signature
	var baseSequence uint64
	s.Require().True(s.Run("Execute Transfer", func() {
		routerStatePDA, _ := solana.Ics26Router.RouterStatePDA(ics26_router.ProgramID)
		clientSequencePDA, _ := solana.Ics26Router.ClientSequenceWithArgSeedPDA(ics26_router.ProgramID, []byte(SolanaClientID))
		gmpAppStatePDA, _ := solana.Ics27Gmp.AppStateGmpportPDA(ics27_gmp.ProgramID)
		gmpIbcAppPDA, _ := solana.Ics26Router.IbcAppWithArgSeedPDA(ics26_router.ProgramID, []byte(IFTPortID))
		ibcClientPDA, _ := solana.Ics26Router.ClientWithArgSeedPDA(ics26_router.ProgramID, []byte(SolanaClientID))

		var err error
		baseSequence, err = s.Solana.Chain.GetNextSequenceNumber(ctx, clientSequencePDA)
		s.Require().NoError(err)

		namespacedSequence := solana.CalculateNamespacedSequence(
			baseSequence,
			ics27_gmp.ProgramID,
			s.SolanaRelayer.PublicKey(),
		)

		seqBytes := make([]byte, 8)
		binary.LittleEndian.PutUint64(seqBytes, namespacedSequence)
		packetCommitmentPDA, _ := solana.Ics26Router.PacketCommitmentWithArgSeedPDA(ics26_router.ProgramID, []byte(SolanaClientID), seqBytes)
		pendingTransferPDA, _ := solana.Ics27Ift.PendingTransferPDA(ics27_ift.ProgramID, s.IFTMint[:], []byte(SolanaClientID), seqBytes)

		solanaClockTime, err := s.Solana.Chain.GetSolanaClockTime(ctx)
		s.Require().NoError(err)

		timeoutTimestamp := solanaClockTime + 900 // 15 minutes

		transferMsg := ics27_ift.Ics27IftStateIftTransferMsg{
			ClientId:         SolanaClientID,
			Receiver:         cosmosUser.FormattedAddress(),
			Amount:           IFTTransferAmount,
			TimeoutTimestamp: timeoutTimestamp,
		}

		transferIx, err := ics27_ift.NewIftTransferInstruction(
			transferMsg,
			s.IFTAppState,
			s.IFTBridge,
			s.IFTMint,
			senderTokenAccount,
			s.SolanaRelayer.PublicKey(),
			s.SolanaRelayer.PublicKey(),
			token.ProgramID,
			solanago.SystemProgramID,
			ics27_gmp.ProgramID,
			gmpAppStatePDA,
			ics26_router.ProgramID,
			routerStatePDA,
			clientSequencePDA,
			packetCommitmentPDA,
			solanago.SysVarInstructionsPubkey,
			gmpIbcAppPDA,
			ibcClientPDA,
			pendingTransferPDA,
		)
		s.Require().NoError(err)

		computeBudgetIx := solana.NewComputeBudgetInstruction(400_000)
		tx, err := s.Solana.Chain.NewTransactionFromInstructions(s.SolanaRelayer.PublicKey(), computeBudgetIx, transferIx)
		s.Require().NoError(err)

		transferTxSig, err = s.Solana.Chain.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentConfirmed, s.SolanaRelayer)
		s.Require().NoError(err)
		s.T().Logf("IFT transfer transaction: %s", transferTxSig.String())
	}))

	s.Require().True(s.Run("Verify tokens burned", func() {
		burnedBalance, err := s.Solana.Chain.GetTokenBalance(ctx, senderTokenAccount)
		s.Require().NoError(err)
		s.Require().Equal(initialBalance-IFTTransferAmount, burnedBalance, "Tokens should be burned after transfer")
	}))

	s.Require().True(s.Run("Verify PendingTransfer PDA exists before relay", func() {
		namespacedSequence := solana.CalculateNamespacedSequence(baseSequence, ics27_gmp.ProgramID, s.SolanaRelayer.PublicKey())
		s.Solana.Chain.VerifyPendingTransferExists(ctx, s.T(), s.Require(), ics27_ift.ProgramID, s.IFTMint, SolanaClientID, namespacedSequence)
	}))

	var cosmosRecvTxHash string
	s.Require().True(s.Run("Relay packet to wfchain (will fail - no IFT bridge registered)", func() {
		var recvRelayTx []byte
		s.Require().True(s.Run("Retrieve relay tx", func() {
			resp, err := s.RelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
				SrcChain:    testvalues.SolanaChainID,
				DstChain:    s.Wfchain.Config().ChainID,
				SourceTxIds: [][]byte{[]byte(transferTxSig.String())},
				SrcClientId: SolanaClientID,
				DstClientId: CosmosClientID,
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Tx)
			recvRelayTx = resp.Tx
		}))

		s.Require().True(s.Run("Submit relay tx to Cosmos", func() {
			receipt := s.MustBroadcastSdkTxBody(ctx, s.Wfchain, s.Cosmos.Users[0], 2_000_000, recvRelayTx)
			s.Require().Equal(uint32(0), receipt.Code, "IBC layer should succeed even if app fails")
			cosmosRecvTxHash = receipt.TxHash
		}))
	}))

	s.Require().True(s.Run("Relay error ack to Solana", func() {
		cosmosRecvTxHashBytes, err := hex.DecodeString(cosmosRecvTxHash)
		s.Require().NoError(err)

		resp, err := s.RelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
			SrcChain:    s.Wfchain.Config().ChainID,
			DstChain:    testvalues.SolanaChainID,
			SourceTxIds: [][]byte{cosmosRecvTxHashBytes},
			SrcClientId: CosmosClientID,
			DstClientId: SolanaClientID,
		})
		s.Require().NoError(err)
		s.Require().NotEmpty(resp.Tx)

		sig, err := s.Solana.Chain.SubmitChunkedRelayPackets(ctx, s.T(), resp, s.SolanaRelayer)
		s.Require().NoError(err)
		s.T().Logf("Error ack relayed: %s", sig)
	}))

	s.Require().True(s.Run("Verify tokens refunded", func() {
		finalBalance, err := s.Solana.Chain.GetTokenBalance(ctx, senderTokenAccount)
		s.Require().NoError(err)
		s.Require().Equal(initialBalance, finalBalance, "Tokens should be refunded after error ack")
	}))

	s.Require().True(s.Run("Verify PendingTransfer PDA closed", func() {
		namespacedSequence := solana.CalculateNamespacedSequence(
			baseSequence,
			ics27_gmp.ProgramID,
			s.SolanaRelayer.PublicKey(),
		)
		s.Solana.Chain.VerifyPendingTransferClosed(ctx, s.T(), s.Require(),
			ics27_ift.ProgramID, s.IFTMint, SolanaClientID, namespacedSequence)
	}))
}
