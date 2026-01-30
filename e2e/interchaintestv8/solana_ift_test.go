package main

import (
	"context"
	"encoding/binary"
	"encoding/hex"
	"testing"
	"time"

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
	ift "github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/ift"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/chainconfig"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/e2esuite"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/solana"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
	relayertypes "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/relayer"
	ifttypes "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/wfchain/ift"
	tokenfactorytypes "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/wfchain/tokenfactory"
)

// IbcEurekaSolanaIFTTestSuite tests IFT functionality
type IbcEurekaSolanaIFTTestSuite struct {
	IbcEurekaSolanaTestSuite

	Wfchain         *cosmos.CosmosChain
	CosmosSubmitter ibc.Wallet

	IFTMintWallet        *solanago.Wallet // Mint keypair (IFT creates the mint during init)
	IFTMintAuthority     solanago.PublicKey
	IFTAppState          solanago.PublicKey
	IFTBridge            solanago.PublicKey
	SenderTokenAccount   solanago.PublicKey
	ReceiverTokenAccount solanago.PublicKey

	CosmosUser ibc.Wallet // Primary user for IFT operations

	GMPAppStatePDA    solanago.PublicKey
	RouterStatePDA    solanago.PublicKey
	IBCClientPDA      solanago.PublicKey
	GMPIBCAppPDA      solanago.PublicKey
	ClientSequencePDA solanago.PublicKey
	AccessManagerPDA  solanago.PublicKey
}

// IFTMint returns the mint public key
func (s *IbcEurekaSolanaIFTTestSuite) IFTMint() solanago.PublicKey {
	return s.IFTMintWallet.PublicKey()
}

// IFTMintBytes returns the mint public key as bytes (for PDA derivation)
func (s *IbcEurekaSolanaIFTTestSuite) IFTMintBytes() []byte {
	pk := s.IFTMintWallet.PublicKey()
	return pk[:]
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

func (s *IbcEurekaSolanaIFTTestSuite) SetupSuite(ctx context.Context) {
	chainconfig.DefaultChainSpecs = []*interchaintest.ChainSpec{
		chainconfig.WfchainChainSpec("wfchain-1", "wfchain-1"),
	}

	s.UseMockWasmClient = true
	s.IbcEurekaSolanaTestSuite.SetupSuite(ctx)

	s.Wfchain = s.Cosmos.Chains[0]
	s.CosmosSubmitter = s.CreateAndFundCosmosUser(ctx, s.Wfchain)
	s.CosmosUser = s.Cosmos.Users[0]

	s.initializeICS27GMP(ctx)

	s.GMPAppStatePDA, _ = solana.Ics27Gmp.AppStateGmpportPDA(ics27_gmp.ProgramID)
	s.RouterStatePDA, _ = solana.Ics26Router.RouterStatePDA(ics26_router.ProgramID)
	s.IBCClientPDA, _ = solana.Ics26Router.ClientWithArgSeedPDA(ics26_router.ProgramID, []byte(SolanaClientID))
	s.GMPIBCAppPDA, _ = solana.Ics26Router.IbcAppWithArgSeedPDA(ics26_router.ProgramID, []byte(GMPPortID))
	s.ClientSequencePDA, _ = solana.Ics26Router.ClientSequenceWithArgSeedPDA(ics26_router.ProgramID, []byte(SolanaClientID))
	s.AccessManagerPDA, _ = solana.AccessManager.AccessManagerPDA(access_manager.ProgramID)
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

// mintTokenFactory mints tokenfactory tokens to a recipient
func (s *IbcEurekaSolanaIFTTestSuite) mintTokenFactory(ctx context.Context, user ibc.Wallet, denom string, amount sdkmath.Int, recipient string) {
	msg := &tokenfactorytypes.MsgMint{
		From:    user.FormattedAddress(),
		Address: recipient,
		Amount:  sdk.Coin{Denom: denom, Amount: amount},
	}
	_, err := s.BroadcastMessages(ctx, s.Wfchain, user, 200_000, msg)
	s.Require().NoError(err)
}

func (s *IbcEurekaSolanaIFTTestSuite) registerCosmosIFTBridge(ctx context.Context, denom, clientId, counterpartyIftAddr, counterpartyClientId string, gmpProgramID, mint solanago.PublicKey) {
	govModuleAddr, err := s.Wfchain.AuthQueryModuleAddress(ctx, govtypes.ModuleName)
	s.Require().NoError(err)

	// counterpartyClientId is the client on Solana that tracks Cosmos - needed for gmp_account_pda derivation
	constructor := testvalues.BuildSolanaIFTConstructor(gmpProgramID.String(), mint.String())
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

// registerSolanaIFTBridge registers an IFT bridge for the Cosmos counterparty
func (s *IbcEurekaSolanaIFTTestSuite) registerSolanaIFTBridge(ctx context.Context, clientID, counterpartyAddress, counterpartyDenom string) {
	s.Require().True(s.Run("Register IFT Bridge", func() {
		bridgePDA, _ := solana.Ift.IftBridgePDA(ift.ProgramID, s.IFTMintBytes(), []byte(clientID))
		s.IFTBridge = bridgePDA

		// Query the ICA address on Cosmos for the IFT program
		// When IFT calls GMP via CPI, the sender is the IFT program ID (not the user)
		// This is similar to Ethereum where the IFT contract address is the sender
		iftProgramAddress := ift.ProgramID.String()
		res, err := e2esuite.GRPCQuery[gmptypes.QueryAccountAddressResponse](ctx, s.Wfchain, &gmptypes.QueryAccountAddressRequest{
			ClientId: CosmosClientID, // The wasm client on Cosmos (dest client)
			Sender:   iftProgramAddress,
			Salt:     "",
		})
		s.Require().NoError(err)
		s.Require().NotEmpty(res.AccountAddress)
		cosmosIcaAddress := res.AccountAddress
		s.T().Logf("Computed Cosmos ICA address: %s (for IFT program: %s)", cosmosIcaAddress, iftProgramAddress)

		registerMsg := ift.IftStateRegisterIftBridgeMsg{
			ClientId:               clientID,
			CounterpartyIftAddress: counterpartyAddress,
			ChainOptions: &ift.IftStateChainOptions_Cosmos{
				Denom:      counterpartyDenom,
				TypeUrl:    "/wfchain.ift.MsgIFTMint", // Type URL for Cosmos MsgIFTMint
				IcaAddress: cosmosIcaAddress,
			},
		}

		registerIx, err := ift.NewRegisterIftBridgeInstruction(
			registerMsg,
			s.IFTAppState,
			bridgePDA,
			s.AccessManagerPDA,
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

// createIFTSplToken creates a new SPL token for IFT
func (s *IbcEurekaSolanaIFTTestSuite) createIFTSplToken(ctx context.Context, mintWallet *solanago.Wallet) {
	mint := mintWallet.PublicKey()
	appStatePDA, _ := solana.Ift.IftAppStatePDA(ift.ProgramID, mint[:])
	mintAuthorityPDA, _ := solana.Ift.IftMintAuthorityPDA(ift.ProgramID, mint[:])

	s.IFTAppState = appStatePDA
	s.IFTMintAuthority = mintAuthorityPDA

	initIx, err := ift.NewCreateSplTokenInstruction(
		IFTTokenDecimals,
		access_manager.ProgramID,
		ics27_gmp.ProgramID,
		appStatePDA,
		mint,
		mintAuthorityPDA,
		s.SolanaRelayer.PublicKey(),
		token.ProgramID,
		solanago.SystemProgramID,
	)
	s.Require().NoError(err)

	tx, err := s.Solana.Chain.NewTransactionFromInstructions(s.SolanaRelayer.PublicKey(), initIx)
	s.Require().NoError(err)

	// Both the payer and mint must sign (mint is created during init)
	_, err = s.Solana.Chain.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentConfirmed, s.SolanaRelayer, mintWallet)
	s.Require().NoError(err)
}

// initializeExistingToken creates an SPL token with relayer as authority, mints tokens,
// then transfers mint authority to IFT.
func (s *IbcEurekaSolanaIFTTestSuite) initializeExistingToken(ctx context.Context, amount uint64) solanago.PublicKey {
	// Generate mint keypair - we need this to create the token
	mintWallet := solanago.NewWallet()
	mint := mintWallet.PublicKey()

	// Create SPL token with relayer as mint authority
	rentExemption, err := s.Solana.Chain.RPCClient.GetMinimumBalanceForRentExemption(ctx, 82, "confirmed")
	s.Require().NoError(err)

	// Build CreateAccount instruction data: [0,0,0,0] (discriminator) + lamports + space + owner
	lamportsBytes := make([]byte, 8)
	binary.LittleEndian.PutUint64(lamportsBytes, rentExemption)
	spaceBytes := make([]byte, 8)
	binary.LittleEndian.PutUint64(spaceBytes, 82) // Mint account size

	createAccountData := append([]byte{0, 0, 0, 0}, lamportsBytes...) // CreateAccount = 0
	createAccountData = append(createAccountData, spaceBytes...)
	createAccountData = append(createAccountData, token.ProgramID[:]...)

	createAccountIx := solanago.NewInstruction(
		solanago.SystemProgramID,
		solanago.AccountMetaSlice{
			solanago.NewAccountMeta(s.SolanaRelayer.PublicKey(), true, true),
			solanago.NewAccountMeta(mint, true, true),
		},
		createAccountData,
	)

	// InitializeMint2 instruction
	initMintIx := token.NewInitializeMint2Instruction(
		IFTTokenDecimals,
		s.SolanaRelayer.PublicKey(), // Mint authority
		s.SolanaRelayer.PublicKey(), // Freeze authority
		mint,
	).Build()

	tx, err := s.Solana.Chain.NewTransactionFromInstructions(s.SolanaRelayer.PublicKey(), createAccountIx, initMintIx)
	s.Require().NoError(err)

	_, err = s.Solana.Chain.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentConfirmed, s.SolanaRelayer, mintWallet)
	s.Require().NoError(err)
	s.T().Logf("Created SPL token mint: %s", mint)

	// Store mint wallet for IFTMint() and IFTMintBytes() methods
	s.IFTMintWallet = mintWallet

	// Create ATA for sender
	senderATA, err := s.Solana.Chain.CreateOrGetAssociatedTokenAccount(ctx, s.SolanaRelayer, mint, s.SolanaRelayer.PublicKey())
	s.Require().NoError(err)
	s.SenderTokenAccount = senderATA

	// Mint tokens to sender
	err = s.Solana.Chain.MintTokensTo(ctx, s.SolanaRelayer, mint, senderATA, amount)
	s.Require().NoError(err)
	s.T().Logf("Minted %d tokens to %s", amount, senderATA)

	// Derive IFT PDAs
	appStatePDA, _ := solana.Ift.IftAppStatePDA(ift.ProgramID, mint[:])
	mintAuthorityPDA, _ := solana.Ift.IftMintAuthorityPDA(ift.ProgramID, mint[:])

	s.IFTAppState = appStatePDA
	s.IFTMintAuthority = mintAuthorityPDA

	// Transfer mint authority to IFT via initialize_existing_token
	initExistingIx, err := ift.NewInitializeExistingTokenInstruction(
		access_manager.ProgramID,
		ics27_gmp.ProgramID,
		appStatePDA,
		mint,
		mintAuthorityPDA,
		s.SolanaRelayer.PublicKey(), // current authority (will sign)
		s.SolanaRelayer.PublicKey(), // payer
		token.ProgramID,
		solanago.SystemProgramID,
	)
	s.Require().NoError(err)

	tx2, err := s.Solana.Chain.NewTransactionFromInstructions(s.SolanaRelayer.PublicKey(), initExistingIx)
	s.Require().NoError(err)

	_, err = s.Solana.Chain.SignAndBroadcastTxWithRetry(ctx, tx2, rpc.CommitmentConfirmed, s.SolanaRelayer)
	s.Require().NoError(err)
	s.T().Logf("Transferred mint authority to IFT PDA: %s", mintAuthorityPDA)

	return mint
}

// Test_IFT_SolanaToCosmosRoundtrip test: Solana → Cosmos → Solana
func (s *IbcEurekaSolanaIFTTestSuite) Test_IFT_SolanaToCosmosRoundtrip() {
	ctx := context.Background()
	s.SetupSuite(ctx)

	var mint solanago.PublicKey
	s.Require().True(s.Run("Create SPL token with initial balance on Solana", func() {
		mint = s.initializeExistingToken(ctx, IFTMintAmount)
		s.T().Logf("SPL token initialized with IFT: %s", mint)
	}))

	// Create tokenfactory denom on Cosmos (no initial mint - IFT will mint when receiving)
	var cosmosDenom string
	s.Require().True(s.Run("Create tokenfactory denom on Cosmos", func() {
		cosmosDenom = s.createTokenFactoryDenom(ctx, testvalues.IFTTestDenom)
	}))

	s.Require().True(s.Run("Register IFT Bridges", func() {
		s.registerCosmosIFTBridge(ctx, cosmosDenom, testvalues.FirstWasmClientID, ift.ProgramID.String(), SolanaClientID, ics27_gmp.ProgramID, mint)
		iftModuleAddr := s.getCosmosIFTModuleAddress()
		s.registerSolanaIFTBridge(ctx, SolanaClientID, iftModuleAddr, cosmosDenom)
	}))

	// === Solana → Cosmos ===
	var solanaToCosmosSequence uint64
	var solanaToCosmosBaseSeq uint64
	var solanaTransferTxSig solanago.Signature
	s.Require().True(s.Run("Transfer: Solana → Cosmos", func() {
		var err error
		solanaToCosmosBaseSeq, err = s.Solana.Chain.GetNextSequenceNumber(ctx, s.ClientSequencePDA)
		s.Require().NoError(err)

		solanaToCosmosSequence = solana.CalculateNamespacedSequence(solanaToCosmosBaseSeq, ics27_gmp.ProgramID, s.SolanaRelayer.PublicKey())
		seqBytes := make([]byte, 8)
		binary.LittleEndian.PutUint64(seqBytes, solanaToCosmosSequence)

		packetCommitmentPDA, _ := solana.Ics26Router.PacketCommitmentWithArgSeedPDA(ics26_router.ProgramID, []byte(SolanaClientID), seqBytes)
		pendingTransferPDA, _ := solana.Ift.PendingTransferPDA(ift.ProgramID, mint[:], []byte(SolanaClientID), seqBytes)

		solanaClockTime, err := s.Solana.Chain.GetSolanaClockTime(ctx)
		s.Require().NoError(err)

		transferMsg := ift.IftStateIftTransferMsg{
			ClientId:         SolanaClientID,
			Receiver:         s.CosmosUser.FormattedAddress(),
			Amount:           IFTTransferAmount,
			TimeoutTimestamp: solanaClockTime + 900,
		}

		transferIx, err := ift.NewIftTransferInstruction(
			transferMsg, s.IFTAppState, s.IFTBridge, mint, s.SenderTokenAccount,
			s.SolanaRelayer.PublicKey(), s.SolanaRelayer.PublicKey(),
			token.ProgramID, solanago.SystemProgramID, ics27_gmp.ProgramID, s.GMPAppStatePDA,
			ics26_router.ProgramID, s.RouterStatePDA, s.ClientSequencePDA, packetCommitmentPDA,
			solanago.SysVarInstructionsPubkey, s.GMPIBCAppPDA, s.IBCClientPDA, pendingTransferPDA,
		)
		s.Require().NoError(err)

		computeBudgetIx := solana.NewComputeBudgetInstruction(400_000)
		tx, err := s.Solana.Chain.NewTransactionFromInstructions(s.SolanaRelayer.PublicKey(), computeBudgetIx, transferIx)
		s.Require().NoError(err)

		solanaTransferTxSig, err = s.Solana.Chain.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentConfirmed, s.SolanaRelayer)
		s.Require().NoError(err)
		s.T().Logf("Solana → Cosmos transfer tx: %s", solanaTransferTxSig)
	}))

	s.Require().True(s.Run("Verify tokens burned on Solana", func() {
		balance, err := s.Solana.Chain.GetTokenBalance(ctx, s.SenderTokenAccount)
		s.Require().NoError(err)
		s.Require().Equal(IFTMintAmount-IFTTransferAmount, balance)
	}))

	var cosmosRecvTxHash string
	s.Require().True(s.Run("Relay to Cosmos", func() {
		resp, err := s.RelayerClient.RelayByTx(ctx, &relayertypes.RelayByTxRequest{
			SrcChain:    testvalues.SolanaChainID,
			DstChain:    s.Wfchain.Config().ChainID,
			SourceTxIds: [][]byte{[]byte(solanaTransferTxSig.String())},
			SrcClientId: SolanaClientID,
			DstClientId: CosmosClientID,
		})
		s.Require().NoError(err)

		receipt := s.MustBroadcastSdkTxBody(ctx, s.Wfchain, s.CosmosUser, 2_000_000, resp.Tx)
		cosmosRecvTxHash = receipt.TxHash
		s.T().Logf("Cosmos recv tx: %s", cosmosRecvTxHash)
	}))

	s.Require().True(s.Run("Verify tokens minted on Cosmos", func() {
		balance, err := s.Wfchain.GetBalance(ctx, s.CosmosUser.FormattedAddress(), cosmosDenom)
		s.Require().NoError(err)
		s.Require().Equal(sdkmath.NewInt(int64(IFTTransferAmount)), balance)
	}))

	s.Require().True(s.Run("Relay ack to Solana", func() {
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

		_, err = s.Solana.Chain.SubmitChunkedRelayPackets(ctx, s.T(), resp, s.SolanaRelayer)
		s.Require().NoError(err)
	}))

	s.Require().True(s.Run("Verify PendingTransfer closed after ack", func() {
		s.Solana.Chain.VerifyPendingTransferClosed(ctx, s.T(), s.Require(), ift.ProgramID, mint, SolanaClientID, solanaToCosmosSequence)
	}))

	// === Cosmos → Solana ===
	var cosmosToSolanaTxHash string
	s.Require().True(s.Run("Transfer: Cosmos → Solana", func() {
		timeout := uint64(time.Now().Add(15 * time.Minute).Unix())

		resp, err := s.BroadcastMessages(ctx, s.Wfchain, s.CosmosUser, 200_000, &ifttypes.MsgIFTTransfer{
			Signer:           s.CosmosUser.FormattedAddress(),
			Denom:            cosmosDenom,
			ClientId:         testvalues.FirstWasmClientID,
			Receiver:         s.SolanaRelayer.PublicKey().String(),
			Amount:           sdkmath.NewInt(int64(IFTTransferAmount)),
			TimeoutTimestamp: timeout,
		})
		s.Require().NoError(err)
		cosmosToSolanaTxHash = resp.TxHash
		s.T().Logf("Cosmos → Solana transfer tx: %s", cosmosToSolanaTxHash)
	}))

	s.Require().True(s.Run("Verify tokens burned on Cosmos", func() {
		balance, err := s.Wfchain.GetBalance(ctx, s.CosmosUser.FormattedAddress(), cosmosDenom)
		s.Require().NoError(err)
		s.Require().True(balance.IsZero())
	}))

	var solanaRecvTxSig solanago.Signature
	s.Require().True(s.Run("Relay to Solana", func() {
		cosmosIFTTxHashBytes, err := hex.DecodeString(cosmosToSolanaTxHash)
		s.Require().NoError(err)

		resp, err := s.RelayerClient.RelayByTx(ctx, &relayertypes.RelayByTxRequest{
			SrcChain:    s.Wfchain.Config().ChainID,
			DstChain:    testvalues.SolanaChainID,
			SourceTxIds: [][]byte{cosmosIFTTxHashBytes},
			SrcClientId: CosmosClientID,
			DstClientId: SolanaClientID,
		})
		s.Require().NoError(err)

		solanaRecvTxSig, err = s.Solana.Chain.SubmitChunkedRelayPackets(ctx, s.T(), resp, s.SolanaRelayer)
		s.Require().NoError(err)
		s.T().Logf("Solana recv tx: %s", solanaRecvTxSig)
	}))

	s.Require().True(s.Run("Verify tokens restored on Solana", func() {
		balance, err := s.Solana.Chain.GetTokenBalance(ctx, s.SenderTokenAccount)
		s.Require().NoError(err)
		s.Require().Equal(IFTMintAmount, balance, "Balance should be restored after roundtrip")
	}))

	s.Require().True(s.Run("Relay ack to Cosmos", func() {
		resp, err := s.RelayerClient.RelayByTx(ctx, &relayertypes.RelayByTxRequest{
			SrcChain:    testvalues.SolanaChainID,
			DstChain:    s.Wfchain.Config().ChainID,
			SourceTxIds: [][]byte{[]byte(solanaRecvTxSig.String())},
			SrcClientId: SolanaClientID,
			DstClientId: CosmosClientID,
		})
		s.Require().NoError(err)
		_ = s.MustBroadcastSdkTxBody(ctx, s.Wfchain, s.CosmosUser, 2_000_000, resp.Tx)
	}))
}

// Test_IFT_CosmosToSolanaRoundtrip test: Cosmos → Solana → Cosmos
func (s *IbcEurekaSolanaIFTTestSuite) Test_IFT_CosmosToSolanaRoundtrip() {
	ctx := context.Background()
	s.SetupSuite(ctx)

	// Create and mint tokens on Cosmos
	var cosmosDenom string
	s.Require().True(s.Run("Create and mint tokenfactory denom", func() {
		cosmosDenom = s.createTokenFactoryDenom(ctx, testvalues.IFTTestDenom)
		s.mintTokenFactory(ctx, s.CosmosSubmitter, cosmosDenom, sdkmath.NewInt(int64(IFTMintAmount)), s.CosmosUser.FormattedAddress())

		balance, err := s.Wfchain.GetBalance(ctx, s.CosmosUser.FormattedAddress(), cosmosDenom)
		s.Require().NoError(err)
		s.Require().Equal(sdkmath.NewInt(int64(IFTMintAmount)), balance)
	}))

	// Create IFT SPL token on Solana (empty - will receive via IFT)
	var solanaTokenAccount solanago.PublicKey
	s.Require().True(s.Run("Create IFT SPL token", func() {
		s.IFTMintWallet = solanago.NewWallet()
		s.createIFTSplToken(ctx, s.IFTMintWallet)

		mint := s.IFTMintWallet.PublicKey()
		tokenAccount, err := s.Solana.Chain.CreateOrGetAssociatedTokenAccount(ctx, s.SolanaRelayer, mint, s.SolanaRelayer.PublicKey())
		s.Require().NoError(err)
		solanaTokenAccount = tokenAccount
		s.SenderTokenAccount = tokenAccount
	}))

	mint := s.IFTMintWallet.PublicKey()
	s.Require().True(s.Run("Register IFT Bridges", func() {
		s.registerCosmosIFTBridge(ctx, cosmosDenom, testvalues.FirstWasmClientID, ift.ProgramID.String(), SolanaClientID, ics27_gmp.ProgramID, mint)
		iftModuleAddr := s.getCosmosIFTModuleAddress()
		s.registerSolanaIFTBridge(ctx, SolanaClientID, iftModuleAddr, cosmosDenom)
	}))

	// === Cosmos → Solana ===
	var cosmosToSolanaTxHash string
	s.Require().True(s.Run("Transfer: Cosmos → Solana", func() {
		timeout := uint64(time.Now().Add(15 * time.Minute).Unix())

		resp, err := s.BroadcastMessages(ctx, s.Wfchain, s.CosmosUser, 200_000, &ifttypes.MsgIFTTransfer{
			Signer:           s.CosmosUser.FormattedAddress(),
			Denom:            cosmosDenom,
			ClientId:         testvalues.FirstWasmClientID,
			Receiver:         s.SolanaRelayer.PublicKey().String(),
			Amount:           sdkmath.NewInt(int64(IFTTransferAmount)),
			TimeoutTimestamp: timeout,
		})
		s.Require().NoError(err)
		cosmosToSolanaTxHash = resp.TxHash
		s.T().Logf("Cosmos → Solana transfer tx: %s", cosmosToSolanaTxHash)
	}))

	s.Require().True(s.Run("Verify tokens burned on Cosmos", func() {
		balance, err := s.Wfchain.GetBalance(ctx, s.CosmosUser.FormattedAddress(), cosmosDenom)
		s.Require().NoError(err)
		expected := sdkmath.NewInt(int64(IFTMintAmount - IFTTransferAmount))
		s.Require().Equal(expected, balance)
	}))

	var solanaRecvTxSig solanago.Signature
	s.Require().True(s.Run("Relay to Solana", func() {
		cosmosIFTTxHashBytes, err := hex.DecodeString(cosmosToSolanaTxHash)
		s.Require().NoError(err)

		resp, err := s.RelayerClient.RelayByTx(ctx, &relayertypes.RelayByTxRequest{
			SrcChain:    s.Wfchain.Config().ChainID,
			DstChain:    testvalues.SolanaChainID,
			SourceTxIds: [][]byte{cosmosIFTTxHashBytes},
			SrcClientId: CosmosClientID,
			DstClientId: SolanaClientID,
		})
		s.Require().NoError(err)

		solanaRecvTxSig, err = s.Solana.Chain.SubmitChunkedRelayPackets(ctx, s.T(), resp, s.SolanaRelayer)
		s.Require().NoError(err)
		s.T().Logf("Solana recv tx: %s", solanaRecvTxSig)
	}))

	s.Require().True(s.Run("Verify tokens minted on Solana", func() {
		balance, err := s.Solana.Chain.GetTokenBalance(ctx, solanaTokenAccount)
		s.Require().NoError(err)
		s.Require().Equal(IFTTransferAmount, balance)
	}))

	s.Require().True(s.Run("Relay ack to Cosmos", func() {
		resp, err := s.RelayerClient.RelayByTx(ctx, &relayertypes.RelayByTxRequest{
			SrcChain:    testvalues.SolanaChainID,
			DstChain:    s.Wfchain.Config().ChainID,
			SourceTxIds: [][]byte{[]byte(solanaRecvTxSig.String())},
			SrcClientId: SolanaClientID,
			DstClientId: CosmosClientID,
		})
		s.Require().NoError(err)
		_ = s.MustBroadcastSdkTxBody(ctx, s.Wfchain, s.CosmosUser, 2_000_000, resp.Tx)
	}))

	// === Solana → Cosmos ===
	var solanaToCosmosSequence uint64
	var solanaToCosmosBaseSeq uint64
	var solanaTransferTxSig solanago.Signature
	s.Require().True(s.Run("Transfer: Solana → Cosmos", func() {
		var err error
		solanaToCosmosBaseSeq, err = s.Solana.Chain.GetNextSequenceNumber(ctx, s.ClientSequencePDA)
		s.Require().NoError(err)

		solanaToCosmosSequence = solana.CalculateNamespacedSequence(solanaToCosmosBaseSeq, ics27_gmp.ProgramID, s.SolanaRelayer.PublicKey())
		seqBytes := make([]byte, 8)
		binary.LittleEndian.PutUint64(seqBytes, solanaToCosmosSequence)

		packetCommitmentPDA, _ := solana.Ics26Router.PacketCommitmentWithArgSeedPDA(ics26_router.ProgramID, []byte(SolanaClientID), seqBytes)
		pendingTransferPDA, _ := solana.Ift.PendingTransferPDA(ift.ProgramID, mint[:], []byte(SolanaClientID), seqBytes)

		solanaClockTime, err := s.Solana.Chain.GetSolanaClockTime(ctx)
		s.Require().NoError(err)

		transferMsg := ift.IftStateIftTransferMsg{
			ClientId:         SolanaClientID,
			Receiver:         s.CosmosUser.FormattedAddress(),
			Amount:           IFTTransferAmount,
			TimeoutTimestamp: solanaClockTime + 900,
		}

		transferIx, err := ift.NewIftTransferInstruction(
			transferMsg, s.IFTAppState, s.IFTBridge, mint, solanaTokenAccount,
			s.SolanaRelayer.PublicKey(), s.SolanaRelayer.PublicKey(),
			token.ProgramID, solanago.SystemProgramID, ics27_gmp.ProgramID, s.GMPAppStatePDA,
			ics26_router.ProgramID, s.RouterStatePDA, s.ClientSequencePDA, packetCommitmentPDA,
			solanago.SysVarInstructionsPubkey, s.GMPIBCAppPDA, s.IBCClientPDA, pendingTransferPDA,
		)
		s.Require().NoError(err)

		computeBudgetIx := solana.NewComputeBudgetInstruction(400_000)
		tx, err := s.Solana.Chain.NewTransactionFromInstructions(s.SolanaRelayer.PublicKey(), computeBudgetIx, transferIx)
		s.Require().NoError(err)

		solanaTransferTxSig, err = s.Solana.Chain.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentConfirmed, s.SolanaRelayer)
		s.Require().NoError(err)
		s.T().Logf("Solana → Cosmos transfer tx: %s", solanaTransferTxSig)
	}))

	s.Require().True(s.Run("Verify tokens burned on Solana", func() {
		balance, err := s.Solana.Chain.GetTokenBalance(ctx, solanaTokenAccount)
		s.Require().NoError(err)
		s.Require().Equal(uint64(0), balance)
	}))

	var cosmosRecvTxHash string
	s.Require().True(s.Run("Relay to Cosmos", func() {
		resp, err := s.RelayerClient.RelayByTx(ctx, &relayertypes.RelayByTxRequest{
			SrcChain:    testvalues.SolanaChainID,
			DstChain:    s.Wfchain.Config().ChainID,
			SourceTxIds: [][]byte{[]byte(solanaTransferTxSig.String())},
			SrcClientId: SolanaClientID,
			DstClientId: CosmosClientID,
		})
		s.Require().NoError(err)

		receipt := s.MustBroadcastSdkTxBody(ctx, s.Wfchain, s.CosmosUser, 2_000_000, resp.Tx)
		cosmosRecvTxHash = receipt.TxHash
		s.T().Logf("Cosmos recv tx: %s", cosmosRecvTxHash)
	}))

	s.Require().True(s.Run("Verify tokens restored on Cosmos", func() {
		balance, err := s.Wfchain.GetBalance(ctx, s.CosmosUser.FormattedAddress(), cosmosDenom)
		s.Require().NoError(err)
		s.Require().Equal(sdkmath.NewInt(int64(IFTMintAmount)), balance, "Balance should be restored after roundtrip")
	}))

	s.Require().True(s.Run("Relay ack to Solana", func() {
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

		_, err = s.Solana.Chain.SubmitChunkedRelayPackets(ctx, s.T(), resp, s.SolanaRelayer)
		s.Require().NoError(err)
	}))

	s.Require().True(s.Run("Verify PendingTransfer closed", func() {
		s.Solana.Chain.VerifyPendingTransferClosed(ctx, s.T(), s.Require(), ift.ProgramID, mint, SolanaClientID, solanaToCosmosSequence)
	}))
}

// Test_IFT_AdminSetupFlow tests IFT initialization creates the mint with correct authority
func (s *IbcEurekaSolanaIFTTestSuite) Test_IFT_AdminSetupFlow() {
	ctx := context.Background()
	s.SetupSuite(ctx)

	s.Require().True(s.Run("Create IFT SPL token (creates mint)", func() {
		s.IFTMintWallet = solanago.NewWallet()
		s.createIFTSplToken(ctx, s.IFTMintWallet)

		mint := s.IFTMint()
		s.T().Logf("SPL Token mint created by IFT: %s", mint.String())
	}))

	var expectedMintAuthority solanago.PublicKey
	s.Require().True(s.Run("Verify mint authority is IFT PDA", func() {
		mint := s.IFTMint()
		expectedMintAuthority, _ = solana.Ift.IftMintAuthorityPDA(ift.ProgramID, mint[:])
		s.Solana.Chain.VerifyMintAuthority(ctx, s.T(), s.Require(), mint, expectedMintAuthority)
		s.T().Logf("Mint authority is IFT PDA: %s", expectedMintAuthority.String())
	}))

	var bridgePDA solanago.PublicKey
	cosmosCounterpartyAddress := "cosmos1test123456789" // Mock counterparty
	s.Require().True(s.Run("Register IFT Bridge", func() {
		s.registerSolanaIFTBridge(ctx, SolanaClientID, cosmosCounterpartyAddress, testvalues.IFTTestDenom)
		bridgePDA = s.IFTBridge
		s.T().Logf("IFT Bridge registered: %s", bridgePDA.String())
	}))

	s.Require().True(s.Run("Verify bridge is active", func() {
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

	var iftMintAuthorityPDA solanago.PublicKey
	s.Require().True(s.Run("Create IFT SPL token", func() {
		s.IFTMintWallet = solanago.NewWallet()
		s.createIFTSplToken(ctx, s.IFTMintWallet)

		mint := s.IFTMint()
		iftMintAuthorityPDA, _ = solana.Ift.IftMintAuthorityPDA(ift.ProgramID, mint[:])
		s.Solana.Chain.VerifyMintAuthority(ctx, s.T(), s.Require(), mint, iftMintAuthorityPDA)
		s.T().Logf("IFT initialized - mint authority: %s", iftMintAuthorityPDA)
	}))

	// Create new wallet to receive mint authority
	newAuthorityWallet, err := s.Solana.Chain.CreateAndFundWallet()
	s.Require().NoError(err)

	s.Require().True(s.Run("Verify app state exists before revoke", func() {
		s.Solana.Chain.VerifyIftAppStateExists(ctx, s.T(), s.Require(), ift.ProgramID, s.IFTMint())
	}))

	s.Require().True(s.Run("Revoke mint authority", func() {
		revokeIx, err := ift.NewRevokeMintAuthorityInstruction(
			s.IFTAppState,
			s.IFTMint(),
			iftMintAuthorityPDA,
			newAuthorityWallet.PublicKey(),
			s.AccessManagerPDA,
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
		s.Solana.Chain.VerifyMintAuthority(ctx, s.T(), s.Require(), s.IFTMint(), newAuthorityWallet.PublicKey())
		s.T().Logf("✓ Mint authority transferred to: %s", newAuthorityWallet.PublicKey())
	}))

	s.Require().True(s.Run("Verify IFT app state closed", func() {
		s.Solana.Chain.VerifyIftAppStateClosed(ctx, s.T(), s.Require(), ift.ProgramID, s.IFTMint())
	}))

	s.Require().True(s.Run("Verify new authority can mint tokens", func() {
		mint := s.IFTMint()
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

	// Create and mint tokenfactory tokens on Cosmos
	var cosmosDenom string
	s.Require().True(s.Run("Create and mint tokenfactory denom", func() {
		cosmosDenom = s.createTokenFactoryDenom(ctx, testvalues.IFTTestDenom)
		s.mintTokenFactory(ctx, s.CosmosSubmitter, cosmosDenom, sdkmath.NewInt(int64(IFTMintAmount)), s.CosmosUser.FormattedAddress())
	}))

	// Create IFT SPL token and create sender token account
	var senderTokenAccount solanago.PublicKey
	s.Require().True(s.Run("Create IFT SPL token", func() {
		s.IFTMintWallet = solanago.NewWallet()
		s.createIFTSplToken(ctx, s.IFTMintWallet)

		mint := s.IFTMint()
		tokenAccount, err := s.Solana.Chain.CreateOrGetAssociatedTokenAccount(ctx, s.SolanaRelayer, mint, s.SolanaRelayer.PublicKey())
		s.Require().NoError(err)
		senderTokenAccount = tokenAccount
		s.SenderTokenAccount = tokenAccount
	}))

	// Register bridges on both sides
	s.Require().True(s.Run("Register IFT Bridges", func() {
		s.registerCosmosIFTBridge(ctx, cosmosDenom, testvalues.FirstWasmClientID, ift.ProgramID.String(), SolanaClientID, ics27_gmp.ProgramID, s.IFTMint())
		iftModuleAddr := s.getCosmosIFTModuleAddress()
		s.registerSolanaIFTBridge(ctx, SolanaClientID, iftModuleAddr, cosmosDenom)
	}))

	// Seed Solana with tokens via transfer from Cosmos
	s.Require().True(s.Run("Seed Solana with tokens from Cosmos", func() {
		timeout := uint64(time.Now().Add(15 * time.Minute).Unix())
		resp, err := s.BroadcastMessages(ctx, s.Wfchain, s.CosmosUser, 200_000, &ifttypes.MsgIFTTransfer{
			Signer:           s.CosmosUser.FormattedAddress(),
			Denom:            cosmosDenom,
			ClientId:         testvalues.FirstWasmClientID,
			Receiver:         s.SolanaRelayer.PublicKey().String(),
			Amount:           sdkmath.NewInt(int64(IFTMintAmount)),
			TimeoutTimestamp: timeout,
		})
		s.Require().NoError(err)

		cosmosIFTTxHashBytes, err := hex.DecodeString(resp.TxHash)
		s.Require().NoError(err)

		relayResp, err := s.RelayerClient.RelayByTx(ctx, &relayertypes.RelayByTxRequest{
			SrcChain:    s.Wfchain.Config().ChainID,
			DstChain:    testvalues.SolanaChainID,
			SourceTxIds: [][]byte{cosmosIFTTxHashBytes},
			SrcClientId: CosmosClientID,
			DstClientId: SolanaClientID,
		})
		s.Require().NoError(err)

		_, err = s.Solana.Chain.SubmitChunkedRelayPackets(ctx, s.T(), relayResp, s.SolanaRelayer)
		s.Require().NoError(err)

		balance, err := s.Solana.Chain.GetTokenBalance(ctx, senderTokenAccount)
		s.Require().NoError(err)
		s.Require().Equal(IFTMintAmount, balance, "Tokens should be minted on Solana")
	}))

	initialBalance, err := s.Solana.Chain.GetTokenBalance(ctx, senderTokenAccount)
	s.Require().NoError(err)

	var solanaPacketTxHash []byte
	var baseSequence uint64
	s.Require().True(s.Run("Execute Transfer with Short Timeout", func() {
		var err error
		baseSequence, err = s.Solana.Chain.GetNextSequenceNumber(ctx, s.ClientSequencePDA)
		s.Require().NoError(err)

		namespacedSequence := solana.CalculateNamespacedSequence(
			baseSequence,
			ics27_gmp.ProgramID,
			s.SolanaRelayer.PublicKey(),
		)

		seqBytes := make([]byte, 8)
		binary.LittleEndian.PutUint64(seqBytes, namespacedSequence)
		packetCommitmentPDA, _ := solana.Ics26Router.PacketCommitmentWithArgSeedPDA(ics26_router.ProgramID, []byte(SolanaClientID), seqBytes)
		pendingTransferPDA, _ := solana.Ift.PendingTransferPDA(ift.ProgramID, s.IFTMintBytes(), []byte(SolanaClientID), seqBytes)

		solanaClockTime, err := s.Solana.Chain.GetSolanaClockTime(ctx)
		s.Require().NoError(err)

		// Use 35 second timeout for faster test execution
		timeoutTimestamp := solanaClockTime + 35
		s.T().Logf("Setting timeout to: %d (solana_clock=%d + 35 seconds)", timeoutTimestamp, solanaClockTime)

		transferMsg := ift.IftStateIftTransferMsg{
			ClientId:         SolanaClientID,
			Receiver:         s.CosmosUser.FormattedAddress(),
			Amount:           IFTTransferAmount,
			TimeoutTimestamp: timeoutTimestamp,
		}

		transferIx, err := ift.NewIftTransferInstruction(
			transferMsg,
			s.IFTAppState,
			s.IFTBridge,
			s.IFTMint(),
			senderTokenAccount,
			s.SolanaRelayer.PublicKey(),
			s.SolanaRelayer.PublicKey(),
			token.ProgramID,
			solanago.SystemProgramID,
			ics27_gmp.ProgramID,
			s.GMPAppStatePDA,
			ics26_router.ProgramID,
			s.RouterStatePDA,
			s.ClientSequencePDA,
			packetCommitmentPDA,
			solanago.SysVarInstructionsPubkey,
			s.GMPIBCAppPDA,
			s.IBCClientPDA,
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
		s.Solana.Chain.VerifyPendingTransferExists(ctx, s.T(), s.Require(), ift.ProgramID, s.IFTMint(), SolanaClientID, namespacedSequence)
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
				ift.ProgramID, s.IFTMint(), SolanaClientID, namespacedSequence)
		}))
	}))
}

// Test_IFT_AckFailureRefund tests that tokens are refunded on acknowledgement failure
// Note: wfchain has IFT module but we intentionally don't register the bridge to trigger error ack
func (s *IbcEurekaSolanaIFTTestSuite) Test_IFT_AckFailureRefund() {
	ctx := context.Background()
	s.SetupSuite(ctx)

	// Create and mint tokenfactory tokens on Cosmos
	var cosmosDenom string
	s.Require().True(s.Run("Create and mint tokenfactory denom", func() {
		cosmosDenom = s.createTokenFactoryDenom(ctx, testvalues.IFTTestDenom)
		s.mintTokenFactory(ctx, s.CosmosSubmitter, cosmosDenom, sdkmath.NewInt(int64(IFTMintAmount)), s.CosmosUser.FormattedAddress())
	}))

	// Create IFT SPL token and create sender token account
	var senderTokenAccount solanago.PublicKey
	s.Require().True(s.Run("Create IFT SPL token", func() {
		s.IFTMintWallet = solanago.NewWallet()
		s.createIFTSplToken(ctx, s.IFTMintWallet)

		mint := s.IFTMint()
		tokenAccount, err := s.Solana.Chain.CreateOrGetAssociatedTokenAccount(ctx, s.SolanaRelayer, mint, s.SolanaRelayer.PublicKey())
		s.Require().NoError(err)
		senderTokenAccount = tokenAccount
		s.SenderTokenAccount = tokenAccount
	}))

	s.Require().True(s.Run("Register bridges and seed tokens", func() {
		s.registerCosmosIFTBridge(ctx, cosmosDenom, testvalues.FirstWasmClientID, ift.ProgramID.String(), SolanaClientID, ics27_gmp.ProgramID, s.IFTMint())
		iftModuleAddr := s.getCosmosIFTModuleAddress()
		s.registerSolanaIFTBridge(ctx, SolanaClientID, iftModuleAddr, cosmosDenom)

		timeout := uint64(time.Now().Add(15 * time.Minute).Unix())
		resp, err := s.BroadcastMessages(ctx, s.Wfchain, s.CosmosUser, 200_000, &ifttypes.MsgIFTTransfer{
			Signer:           s.CosmosUser.FormattedAddress(),
			Denom:            cosmosDenom,
			ClientId:         testvalues.FirstWasmClientID,
			Receiver:         s.SolanaRelayer.PublicKey().String(),
			Amount:           sdkmath.NewInt(int64(IFTMintAmount)),
			TimeoutTimestamp: timeout,
		})
		s.Require().NoError(err)

		cosmosIFTTxHashBytes, err := hex.DecodeString(resp.TxHash)
		s.Require().NoError(err)

		relayResp, err := s.RelayerClient.RelayByTx(ctx, &relayertypes.RelayByTxRequest{
			SrcChain:    s.Wfchain.Config().ChainID,
			DstChain:    testvalues.SolanaChainID,
			SourceTxIds: [][]byte{cosmosIFTTxHashBytes},
			SrcClientId: CosmosClientID,
			DstClientId: SolanaClientID,
		})
		s.Require().NoError(err)

		solanaRecvSig, err := s.Solana.Chain.SubmitChunkedRelayPackets(ctx, s.T(), relayResp, s.SolanaRelayer)
		s.Require().NoError(err)

		balance, err := s.Solana.Chain.GetTokenBalance(ctx, senderTokenAccount)
		s.Require().NoError(err)
		s.Require().Equal(IFTMintAmount, balance, "Tokens should be minted on Solana")

		// Relay ack back to Cosmos to clear pending transfer
		ackResp, err := s.RelayerClient.RelayByTx(ctx, &relayertypes.RelayByTxRequest{
			SrcChain:    testvalues.SolanaChainID,
			DstChain:    s.Wfchain.Config().ChainID,
			SourceTxIds: [][]byte{[]byte(solanaRecvSig.String())},
			SrcClientId: SolanaClientID,
			DstClientId: CosmosClientID,
		})
		s.Require().NoError(err)
		s.MustBroadcastSdkTxBody(ctx, s.Wfchain, s.CosmosUser, 2_000_000, ackResp.Tx)
		s.T().Log("Seeding ack relayed back to Cosmos")
	}))

	s.Require().True(s.Run("Unregister Cosmos bridge to trigger error ack", func() {
		govModuleAddr := authtypes.NewModuleAddress(govtypes.ModuleName).String()
		msg := &ifttypes.MsgRemoveIFTBridge{
			Signer:   govModuleAddr,
			Denom:    cosmosDenom,
			ClientId: testvalues.FirstWasmClientID,
		}
		err := s.ExecuteGovV1Proposal(ctx, msg, s.Wfchain, s.CosmosSubmitter)
		s.Require().NoError(err)
	}))

	initialBalance, err := s.Solana.Chain.GetTokenBalance(ctx, senderTokenAccount)
	s.Require().NoError(err)

	var transferTxSig solanago.Signature
	var baseSequence uint64
	s.Require().True(s.Run("Execute Transfer", func() {
		var err error
		baseSequence, err = s.Solana.Chain.GetNextSequenceNumber(ctx, s.ClientSequencePDA)
		s.Require().NoError(err)

		namespacedSequence := solana.CalculateNamespacedSequence(
			baseSequence,
			ics27_gmp.ProgramID,
			s.SolanaRelayer.PublicKey(),
		)

		seqBytes := make([]byte, 8)
		binary.LittleEndian.PutUint64(seqBytes, namespacedSequence)
		packetCommitmentPDA, _ := solana.Ics26Router.PacketCommitmentWithArgSeedPDA(ics26_router.ProgramID, []byte(SolanaClientID), seqBytes)
		pendingTransferPDA, _ := solana.Ift.PendingTransferPDA(ift.ProgramID, s.IFTMintBytes(), []byte(SolanaClientID), seqBytes)

		solanaClockTime, err := s.Solana.Chain.GetSolanaClockTime(ctx)
		s.Require().NoError(err)

		timeoutTimestamp := solanaClockTime + 900 // 15 minutes

		transferMsg := ift.IftStateIftTransferMsg{
			ClientId:         SolanaClientID,
			Receiver:         s.CosmosUser.FormattedAddress(),
			Amount:           IFTTransferAmount,
			TimeoutTimestamp: timeoutTimestamp,
		}

		transferIx, err := ift.NewIftTransferInstruction(
			transferMsg,
			s.IFTAppState,
			s.IFTBridge,
			s.IFTMint(),
			senderTokenAccount,
			s.SolanaRelayer.PublicKey(),
			s.SolanaRelayer.PublicKey(),
			token.ProgramID,
			solanago.SystemProgramID,
			ics27_gmp.ProgramID,
			s.GMPAppStatePDA,
			ics26_router.ProgramID,
			s.RouterStatePDA,
			s.ClientSequencePDA,
			packetCommitmentPDA,
			solanago.SysVarInstructionsPubkey,
			s.GMPIBCAppPDA,
			s.IBCClientPDA,
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
		s.Solana.Chain.VerifyPendingTransferExists(ctx, s.T(), s.Require(), ift.ProgramID, s.IFTMint(), SolanaClientID, namespacedSequence)
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
			ift.ProgramID, s.IFTMint(), SolanaClientID, namespacedSequence)
	}))
}
