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

	gmptypes "github.com/cosmos/ibc-go/v10/modules/apps/27-gmp/types"

	access_manager "github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/accessmanager"
	ics26_router "github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/ics26router"
	ics27_gmp "github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/ics27gmp"
	ics27_ift "github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/ics27ift"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/solana"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
	relayertypes "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/relayer"
	solanatypes "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/solana"
)

// IbcEurekaSolanaIFTTestSuite tests ICS27-IFT (Interchain Fungible Token) functionality
type IbcEurekaSolanaIFTTestSuite struct {
	IbcEurekaSolanaTestSuite

	// IFT-specific state
	IFTMint              solanago.PublicKey // SPL token mint controlled by IFT
	IFTMintAuthority     solanago.PublicKey // IFT mint authority PDA
	IFTAppState          solanago.PublicKey // IFT app state PDA
	IFTBridge            solanago.PublicKey // IFT bridge PDA for Cosmos
	SenderTokenAccount   solanago.PublicKey // Sender's token account
	ReceiverTokenAccount solanago.PublicKey // Receiver's token account
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

// registerIFTBridge registers an IFT bridge for the Cosmos counterparty
func (s *IbcEurekaSolanaIFTTestSuite) registerIFTBridge(ctx context.Context, clientID string, counterpartyAddress string) {
	s.Require().True(s.Run("Register IFT Bridge", func() {
		bridgePDA, _ := solana.Ics27Ift.IftBridgePDA(ics27_ift.ProgramID, s.IFTMint[:], []byte(clientID))
		s.IFTBridge = bridgePDA

		accessManagerPDA, _ := solana.AccessManager.AccessManagerPDA(access_manager.ProgramID)

		registerMsg := ics27_ift.Ics27IftStateRegisterIftBridgeMsg{
			ClientId:               clientID,
			CounterpartyIftAddress: counterpartyAddress,
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

// Test_IFT_SolanaToCosmosTransfer tests sending IFT tokens from Solana to Cosmos
func (s *IbcEurekaSolanaIFTTestSuite) Test_IFT_SolanaToCosmosTransfer() {
	ctx := context.Background()

	s.UseMockWasmClient = true
	s.SetupSuite(ctx)

	s.initializeICS27GMP(ctx)

	var senderTokenAccount solanago.PublicKey
	s.Require().True(s.Run("Create SPL Token and Mint Initial Tokens", func() {
		mint, err := s.Solana.Chain.CreateSPLTokenMint(ctx, s.SolanaRelayer, IFTTokenDecimals)
		s.Require().NoError(err)
		s.IFTMint = mint

		tokenAccount, err := s.Solana.Chain.CreateOrGetAssociatedTokenAccount(ctx, s.SolanaRelayer, mint, s.SolanaRelayer.PublicKey())
		s.Require().NoError(err)
		senderTokenAccount = tokenAccount
		s.SenderTokenAccount = tokenAccount

		// Mint before IFT takes over mint authority
		err = s.Solana.Chain.MintTokensTo(ctx, s.SolanaRelayer, mint, tokenAccount, IFTMintAmount)
		s.Require().NoError(err)

		balance, err := s.Solana.Chain.GetTokenBalance(ctx, tokenAccount)
		s.Require().NoError(err)
		s.Require().Equal(IFTMintAmount, balance)
	}))

	s.initializeIFT(ctx, s.IFTMint)
	cosmosUser := s.Cosmos.Users[0]
	s.registerIFTBridge(ctx, SolanaClientID, cosmosUser.FormattedAddress())

	var initialBalance uint64
	s.Require().True(s.Run("Get Initial Balance", func() {
		var err error
		initialBalance, err = s.Solana.Chain.GetTokenBalance(ctx, senderTokenAccount)
		s.Require().NoError(err)
	}))

	s.Require().True(s.Run("Execute IFT Transfer", func() {
		gmpAppStatePDA, _ := solana.Ics27Gmp.AppStateGmpportPDA(ics27_gmp.ProgramID)
		routerStatePDA, _ := solana.Ics26Router.RouterStatePDA(ics26_router.ProgramID)
		ibcClientPDA, _ := solana.Ics26Router.ClientWithArgSeedPDA(ics26_router.ProgramID, []byte(SolanaClientID))
		gmpIbcAppPDA, _ := solana.Ics26Router.IbcAppWithArgSeedPDA(ics26_router.ProgramID, []byte(GMPPortID))
		clientSequencePDA, _ := solana.Ics26Router.ClientSequenceWithArgSeedPDA(ics26_router.ProgramID, []byte(SolanaClientID))

		var packetCommitmentPDA solanago.PublicKey
		baseSequence, err := s.Solana.Chain.GetNextSequenceNumber(ctx, clientSequencePDA)
		s.Require().NoError(err)

		namespacedSequence := solana.CalculateNamespacedSequence(
			baseSequence,
			ics27_gmp.ProgramID,
			s.SolanaRelayer.PublicKey(),
		)

		namespacedSequenceBytes := make([]byte, 8)
		binary.LittleEndian.PutUint64(namespacedSequenceBytes, namespacedSequence)
		packetCommitmentPDA, _ = solana.Ics26Router.PacketCommitmentWithArgSeedPDA(ics26_router.ProgramID, []byte(SolanaClientID), namespacedSequenceBytes)

		pendingTransferPDA, _ := solana.Ics27Ift.PendingTransferWithAccountSeedPDA(ics27_ift.ProgramID, s.IFTMint[:], []byte(SolanaClientID), namespacedSequenceBytes)

		timeoutTimestamp := time.Now().Add(15 * time.Minute).Unix()

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

		// IFT→GMP→Router CPI chain needs more than the default 200k CUs
		computeBudgetIx := solana.NewComputeBudgetInstruction(400_000)
		tx, err := s.Solana.Chain.NewTransactionFromInstructions(s.SolanaRelayer.PublicKey(), computeBudgetIx, transferIx)
		s.Require().NoError(err)

		_, err = s.Solana.Chain.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentConfirmed, s.SolanaRelayer)
		s.Require().NoError(err)
	}))

	s.Require().True(s.Run("Verify Token Burn", func() {
		finalBalance, err := s.Solana.Chain.GetTokenBalance(ctx, senderTokenAccount)
		s.Require().NoError(err)
		expectedBalance := initialBalance - IFTTransferAmount
		s.Require().Equal(expectedBalance, finalBalance, "Tokens should be burned")
	}))
}

// Test_IFT_CosmosToSolanaTransfer tests receiving IFT tokens from Cosmos to Solana
func (s *IbcEurekaSolanaIFTTestSuite) Test_IFT_CosmosToSolanaTransfer() {
	ctx := context.Background()

	s.UseMockWasmClient = true
	s.SetupSuite(ctx)

	simd := s.Cosmos.Chains[0]

	s.initializeICS27GMP(ctx)

	s.Require().True(s.Run("Create SPL Token", func() {
		mint, err := s.Solana.Chain.CreateSPLTokenMint(ctx, s.SolanaRelayer, IFTTokenDecimals)
		s.Require().NoError(err)
		s.IFTMint = mint
	}))

	s.initializeIFT(ctx, s.IFTMint)

	cosmosUser := s.Cosmos.Users[0]
	s.registerIFTBridge(ctx, SolanaClientID, cosmosUser.FormattedAddress())

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

		resp, err := s.BroadcastMessages(ctx, simd, cosmosUser, 2_000_000, &gmptypes.MsgSendCall{
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
			SrcChain:    simd.Config().ChainID,
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
				DstChain:    simd.Config().ChainID,
				SourceTxIds: [][]byte{[]byte(solanaRelayTxSig.String())},
				SrcClientId: SolanaClientID,
				DstClientId: CosmosClientID,
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Tx)
			ackRelayTxBodyBz = resp.Tx
		}))

		s.Require().True(s.Run("Broadcast acknowledgment on Cosmos", func() {
			_ = s.MustBroadcastSdkTxBody(ctx, simd, cosmosUser, CosmosDefaultGasLimit, ackRelayTxBodyBz)
		}))
	}))
}

// Test_IFT_TimeoutRefund tests that tokens are refunded on timeout
func (s *IbcEurekaSolanaIFTTestSuite) Test_IFT_TimeoutRefund() {
	ctx := context.Background()

	s.UseMockWasmClient = true
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
	s.registerIFTBridge(ctx, SolanaClientID, cosmosUser.FormattedAddress())

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
		pendingTransferPDA, _ := solana.Ics27Ift.PendingTransferWithAccountSeedPDA(ics27_ift.ProgramID, s.IFTMint[:], []byte(SolanaClientID), seqBytes)

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

	// Sleep for 40 seconds to let the packet timeout (timeout is set to solana_time + 35 seconds)
	s.T().Log("Sleeping 40 seconds to let packet timeout...")
	time.Sleep(40 * time.Second)

	simd := s.Cosmos.Chains[0]
	s.Require().True(s.Run("Relay timeout back to Solana", func() {
		resp, err := s.RelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
			SrcChain:     simd.Config().ChainID,
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
	}))
}

// Test_IFT_AckFailureRefund tests that tokens are refunded on acknowledgement failure
func (s *IbcEurekaSolanaIFTTestSuite) Test_IFT_AckFailureRefund() {
	ctx := context.Background()

	s.UseMockWasmClient = true
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
	s.registerIFTBridge(ctx, SolanaClientID, cosmosUser.FormattedAddress())

	initialBalance, err := s.Solana.Chain.GetTokenBalance(ctx, senderTokenAccount)
	s.Require().NoError(err)

	var transferTxSig solanago.Signature
	s.Require().True(s.Run("Execute Transfer", func() {
		routerStatePDA, _ := solana.Ics26Router.RouterStatePDA(ics26_router.ProgramID)
		clientSequencePDA, _ := solana.Ics26Router.ClientSequenceWithArgSeedPDA(ics26_router.ProgramID, []byte(SolanaClientID))
		gmpAppStatePDA, _ := solana.Ics27Gmp.AppStateGmpportPDA(ics27_gmp.ProgramID)
		gmpIbcAppPDA, _ := solana.Ics26Router.IbcAppWithArgSeedPDA(ics26_router.ProgramID, []byte(IFTPortID))
		ibcClientPDA, _ := solana.Ics26Router.ClientWithArgSeedPDA(ics26_router.ProgramID, []byte(SolanaClientID))

		baseSequence, err := s.Solana.Chain.GetNextSequenceNumber(ctx, clientSequencePDA)
		s.Require().NoError(err)

		namespacedSequence := solana.CalculateNamespacedSequence(
			baseSequence,
			ics27_gmp.ProgramID,
			s.SolanaRelayer.PublicKey(),
		)

		seqBytes := make([]byte, 8)
		binary.LittleEndian.PutUint64(seqBytes, namespacedSequence)
		packetCommitmentPDA, _ := solana.Ics26Router.PacketCommitmentWithArgSeedPDA(ics26_router.ProgramID, []byte(SolanaClientID), seqBytes)
		pendingTransferPDA, _ := solana.Ics27Ift.PendingTransferWithAccountSeedPDA(ics27_ift.ProgramID, s.IFTMint[:], []byte(SolanaClientID), seqBytes)

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

	simd := s.Cosmos.Chains[0]

	var cosmosRecvTxHash string
	s.Require().True(s.Run("Relay packet to Cosmos (will fail - no IFT handler)", func() {
		var recvRelayTx []byte
		s.Require().True(s.Run("Retrieve relay tx", func() {
			resp, err := s.RelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
				SrcChain:    testvalues.SolanaChainID,
				DstChain:    simd.Config().ChainID,
				SourceTxIds: [][]byte{[]byte(transferTxSig.String())},
				SrcClientId: SolanaClientID,
				DstClientId: CosmosClientID,
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Tx)
			recvRelayTx = resp.Tx
		}))

		s.Require().True(s.Run("Submit relay tx to Cosmos", func() {
			receipt := s.MustBroadcastSdkTxBody(ctx, simd, s.Cosmos.Users[0], 2_000_000, recvRelayTx)
			s.Require().Equal(uint32(0), receipt.Code, "IBC layer should succeed even if app fails")
			cosmosRecvTxHash = receipt.TxHash
		}))
	}))

	s.Require().True(s.Run("Relay error ack to Solana", func() {
		cosmosRecvTxHashBytes, err := hex.DecodeString(cosmosRecvTxHash)
		s.Require().NoError(err)

		resp, err := s.RelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
			SrcChain:    simd.Config().ChainID,
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
}
