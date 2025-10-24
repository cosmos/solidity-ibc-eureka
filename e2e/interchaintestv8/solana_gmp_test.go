package main

import (
	"context"
	"encoding/binary"
	"encoding/hex"
	"fmt"
	"time"

	"github.com/cosmos/gogoproto/proto"
	gmp_counter_app "github.com/cosmos/solidity-ibc-eureka/e2e/interchaintestv8/solana/go-anchor/gmpcounter"
	bin "github.com/gagliardetto/binary"

	solanago "github.com/gagliardetto/solana-go"
	"github.com/gagliardetto/solana-go/programs/system"
	"github.com/gagliardetto/solana-go/programs/token"

	sdkmath "cosmossdk.io/math"

	sdk "github.com/cosmos/cosmos-sdk/types"
	banktypes "github.com/cosmos/cosmos-sdk/x/bank/types"

	gmptypes "github.com/cosmos/ibc-go/v10/modules/apps/27-gmp/types"
	channeltypesv2 "github.com/cosmos/ibc-go/v10/modules/core/04-channel/v2/types"

	"github.com/cosmos/interchaintest/v10/ibc"

	ics26_router "github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/ics26router"
	ics27_gmp "github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/ics27gmp"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/e2esuite"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/solana"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/gmphelpers"
	relayertypes "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/relayer"
	solanatypes "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/solana"
)

const (
	// GMP App
	DefaultIncrementAmount = uint64(5)
	DefaultDecrementAmount = uint64(2)
	GMPPortID              = testvalues.SolanaGMPPortID
	// SPL Token amounts (with 6 decimals)
	SPLTokenDecimals       = uint8(6)
	SPLTokenMintAmount     = uint64(10_000_000) // 10 tokens
	SPLTokenTransferAmount = uint64(1_000_000)  // 1 token
	// Test amounts
	CosmosTestAmount = int64(1000) // stake denom
)

func (s *IbcEurekaSolanaTestSuite) deployAndInitializeGMPCounterApp(ctx context.Context) solanago.PublicKey {
	var gmpCounterProgramID solanago.PublicKey

	s.Require().True(s.Run("Deploy and Initialize GMP Counter App", func() {
		gmpCounterProgramID = s.SolanaChain.DeploySolanaProgram(ctx, s.T(), s.Require(), "gmp_counter_app")

		gmp_counter_app.ProgramID = gmpCounterProgramID

		programAvailable := s.SolanaChain.WaitForProgramAvailabilityWithTimeout(ctx, gmpCounterProgramID, 120)
		s.Require().True(programAvailable, "GMP Counter program failed to become available within timeout")

		// Initialize GMP counter app state
		counterAppStatePDA, _ := solana.CounterAppStatePDA(gmpCounterProgramID)

		initInstruction, err := gmp_counter_app.NewInitializeInstruction(
			s.SolanaUser.PublicKey(), // authority
			counterAppStatePDA,
			s.SolanaUser.PublicKey(), // payer
			solanago.SystemProgramID,
		)
		s.Require().NoError(err)

		tx, err := s.SolanaChain.NewTransactionFromInstructions(s.SolanaUser.PublicKey(), initInstruction)
		s.Require().NoError(err)

		_, err = s.SolanaChain.SignAndBroadcastTxWithRetry(ctx, tx, s.SolanaUser)
		s.Require().NoError(err)
		s.T().Logf("GMP Counter app initialized")
	}))

	return gmpCounterProgramID
}

func (s *IbcEurekaSolanaTestSuite) deployAndInitializeICS27GMP(ctx context.Context) solanago.PublicKey {
	var ics27GMPProgramID solanago.PublicKey

	s.Require().True(s.Run("Deploy and Initialize ICS27 GMP Program", func() {
		ics27GMPProgramID = s.SolanaChain.DeploySolanaProgram(ctx, s.T(), s.Require(), "ics27_gmp")

		// Set the program ID in the bindings
		ics27_gmp.ProgramID = ics27GMPProgramID

		programAvailable := s.SolanaChain.WaitForProgramAvailabilityWithTimeout(ctx, ics27GMPProgramID, 120)
		s.Require().True(programAvailable, "ICS27 GMP program failed to become available within timeout")

		// Find GMP app state PDA (using standard pattern with port_id)
		gmpAppStatePDA, _ := solana.GMPAppStatePDA(GMPPortID)

		// Find router caller PDA
		routerCallerPDA, _ := solana.GMPRouterCallerPDA()

		// Initialize ICS27 GMP app using the actual generated bindings
		// Using GMP port for proper GMP functionality
		initInstruction, err := ics27_gmp.NewInitializeInstruction(
			ics26_router.ProgramID,   // router program
			gmpAppStatePDA,           // app state account
			routerCallerPDA,          // router caller account
			s.SolanaUser.PublicKey(), // payer
			s.SolanaUser.PublicKey(), // authority
			solanago.SystemProgramID, // system program
		)
		s.Require().NoError(err)

		tx, err := s.SolanaChain.NewTransactionFromInstructions(s.SolanaUser.PublicKey(), initInstruction)
		s.Require().NoError(err)

		_, err = s.SolanaChain.SignAndBroadcastTxWithRetry(ctx, tx, s.SolanaUser)
		s.Require().NoError(err)

		s.T().Logf("ICS27 GMP program initialized at: %s", ics27GMPProgramID)
		s.T().Logf("GMP app state PDA: %s", gmpAppStatePDA)
		s.T().Logf("GMP port ID: %s (using proper GMP port)", GMPPortID)
	}))

	// Register GMP app with ICS26 router
	s.Require().True(s.Run("Register ICS27 GMP with Router", func() {
		routerStateAccount, _ := solana.RouterStatePDA()

		ibcAppAccount, _ := solana.RouterIBCAppPDA(GMPPortID)

		registerInstruction, err := ics26_router.NewAddIbcAppInstruction(
			GMPPortID,
			routerStateAccount,
			ibcAppAccount,
			ics27GMPProgramID,
			s.SolanaUser.PublicKey(),
			s.SolanaUser.PublicKey(),
			solanago.SystemProgramID,
		)
		s.Require().NoError(err)

		tx, err := s.SolanaChain.NewTransactionFromInstructions(s.SolanaUser.PublicKey(), registerInstruction)
		s.Require().NoError(err)

		_, err = s.SolanaChain.SignAndBroadcastTxWithRetry(ctx, tx, s.SolanaUser)
		s.Require().NoError(err)
		s.T().Logf("ICS27 GMP registered with router on port: %s (using proper GMP port)", GMPPortID)
	}))

	return ics27GMPProgramID
}

func (s *IbcEurekaSolanaTestSuite) registerGMPCounterAppWithRouter(_ context.Context, gmpCounterProgramID solanago.PublicKey) {
	s.Require().True(s.Run("Setup GMP Counter App as Target", func() {
		// The counter app is now ready to be called via GMP
		// ICS27 GMP will route execution calls to this program based on the receiver field in packets
		s.T().Logf("GMP Counter app %s is ready for GMP execution", gmpCounterProgramID)
		s.T().Logf("Counter app will be callable via GMP packets with receiver = %s", gmpCounterProgramID)
		s.T().Logf("GMP flow: IBC Packet → Router → ICS27 GMP → Counter App")
	}))
}

// Test_GMPCounterFromCosmos tests sending a counter increment call from Cosmos to Solana
func (s *IbcEurekaSolanaTestSuite) Test_GMPCounterFromCosmos() {
	ctx := context.Background()

	s.UseMockWasmClient = true
	s.SetupGMP = true

	s.SetupSuite(ctx)

	simd := s.CosmosChains[0]

	// Create a second Cosmos user for multi-user testing
	var cosmosUser1 ibc.Wallet
	s.Require().True(s.Run("Create Second Cosmos User", func() {
		cosmosUser1 = s.CreateAndFundCosmosUser(ctx, simd)
		s.CosmosUsers = append(s.CosmosUsers, cosmosUser1)
		s.T().Logf("Created second Cosmos user: %s", cosmosUser1.FormattedAddress())
	}))

	// ICS27 GMP program is already deployed and initialized in SetupSuite
	ics27GMPProgramID := ics27_gmp.ProgramID
	s.Require().True(s.Run("Verify ICS27 GMP Program", func() {
	}))

	// Deploy and initialize GMP counter app, then register it with router
	var gmpCounterProgramID solanago.PublicKey
	s.Require().True(s.Run("Deploy and Initialize GMP Counter App", func() {
		gmpCounterProgramID = s.deployAndInitializeGMPCounterApp(ctx)
	}))

	s.Require().True(s.Run("Register GMP Counter App with Router", func() {
		s.registerGMPCounterAppWithRouter(ctx, gmpCounterProgramID)
	}))

	_ = ics27GMPProgramID // Use the GMP program ID for future packet flow

	// Setup user identities and helper functions
	var getCounterValue func(cosmosUserAddress string) uint64
	var sendGMPIncrement func(cosmosUser ibc.Wallet, amount uint64) []byte
	var relayGMPPacket func(cosmosGMPTxHash []byte, userLabel string) solanago.Signature

	s.Require().True(s.Run("Setup User Identities and Helpers", func() {
		// We don't need separate Solana user keys - the ICS27 account_state PDAs are the identities
		// The user counter PDAs are derived from the ICS27 account_state PDAs

		// Helper to get counter value for a Cosmos user
		// This derives the ICS27 account_state PDA, then the user counter PDA from that
		getCounterValue = func(cosmosUserAddress string) uint64 {
			// Derive ICS27 account_state PDA for this Cosmos user
			salt := []byte{} // Empty salt for this test

			ics27AccountPDA, _ := solana.GMPAccountPDA(CosmosClientID, cosmosUserAddress, salt)

			// Derive user counter PDA from ICS27 account_state PDA
			userCounterPDA, _ := solana.CounterUserCounterPDA(ics27AccountPDA, gmpCounterProgramID)

			account, err := s.SolanaChain.RPCClient.GetAccountInfo(ctx, userCounterPDA)
			if err != nil || account.Value == nil {
				return 0 // Account doesn't exist yet
			}

			data := account.Value.Data.GetBinary()
			if len(data) >= 48 {
				return binary.LittleEndian.Uint64(data[40:48])
			}
			return 0
		}

		// Helper to send GMP increment from a Cosmos user
		sendGMPIncrement = func(cosmosUser ibc.Wallet, amount uint64) []byte {
			timeout := uint64(time.Now().Add(30 * time.Minute).Unix())
			simd := s.CosmosChains[0]

			// Derive the ICS27 account_state PDA for this Cosmos user
			// This PDA is the authority that signs for the counter operations
			cosmosAddress := cosmosUser.FormattedAddress()
			salt := []byte{} // Empty salt for this test

			ics27AccountPDA, _ := solana.GMPAccountPDA(CosmosClientID, cosmosAddress, salt)

			// Create the raw instruction data (just discriminator + amount, no user pubkey)
			incrementInstructionData := []byte{}
			incrementInstructionData = append(incrementInstructionData, gmp_counter_app.Instruction_Increment[:]...)
			amountBytes := make([]byte, 8)
			binary.LittleEndian.PutUint64(amountBytes, amount)
			incrementInstructionData = append(incrementInstructionData, amountBytes...)

			// Derive required account addresses
			// 1. Counter app_state PDA
			counterAppStateAddress, _ := solana.CounterAppStatePDA(gmpCounterProgramID)

			// 2. User counter PDA - derived from the ICS27 account_state PDA (not userKey)
			userCounterAddress, _ := solana.CounterUserCounterPDA(ics27AccountPDA, gmpCounterProgramID)

			// Create SolanaInstruction protobuf message
			// Note: PayerPosition = 3 means inject at index 3 (0-indexed)
			// The payer (relayer) is injected by GMP program since Cosmos doesn't know relayer's address
			payerPosition := uint32(3)
			solanaInstruction := &solanatypes.SolanaInstruction{
				ProgramId: gmpCounterProgramID.Bytes(),
				Data:      incrementInstructionData,
				Accounts: []*solanatypes.SolanaAccountMeta{
					// Required accounts for increment instruction (matches IncrementCounter struct order)
					{Pubkey: counterAppStateAddress.Bytes(), IsSigner: false, IsWritable: true}, // [0] counter app_state
					{Pubkey: userCounterAddress.Bytes(), IsSigner: false, IsWritable: true},     // [1] user_counter
					{Pubkey: ics27AccountPDA.Bytes(), IsSigner: true, IsWritable: false},        // [2] user_authority (ICS27 account_state PDA signs via invoke_signed)
					// [3] payer will be injected at index 3 by GMP program
					{Pubkey: solanago.SystemProgramID.Bytes(), IsSigner: false, IsWritable: false}, // [4] system_program (shifts to index 4)
				},
				PayerPosition: &payerPosition, // Inject at index 3 (between user_authority and system_program)
			}

			// Marshal to protobuf bytes
			payload, err := proto.Marshal(solanaInstruction)
			if err != nil {
				return nil
			}

			// Send GMP call using proper gmptypes.MsgSendCall
			resp, err := s.BroadcastMessages(ctx, simd, cosmosUser, 2_000_000, &gmptypes.MsgSendCall{
				SourceClient:     CosmosClientID,
				Sender:           cosmosUser.FormattedAddress(),
				Receiver:         gmpCounterProgramID.String(),
				Salt:             []byte{},
				Payload:          payload,
				TimeoutTimestamp: timeout,
				Memo:             "increment counter via GMP",
				Encoding:         testvalues.Ics27ProtobufEncoding,
			})
			if err != nil {
				return nil
			}

			cosmosGMPTxHashBytes, err := hex.DecodeString(resp.TxHash)
			if err != nil {
				return nil
			}

			s.T().Logf("GMP packet sent from %s: %s (increment by %d)", cosmosUser.FormattedAddress(), resp.TxHash, amount)
			return cosmosGMPTxHashBytes
		}

		// Helper to relay and execute a GMP packet
		relayGMPPacket = func(cosmosGMPTxHash []byte, userLabel string) solanago.Signature {
			var solanaRelayTxSig solanago.Signature

			simd := s.CosmosChains[0]

			// First, update the Solana client to the latest height
			updateResp, err := s.RelayerClient.UpdateClient(context.Background(), &relayertypes.UpdateClientRequest{
				SrcChain:    simd.Config().ChainID,
				DstChain:    testvalues.SolanaChainID,
				DstClientId: SolanaClientID,
			})
			s.Require().NoError(err, "Relayer Update Client failed")
			s.Require().NotEmpty(updateResp.Tx, "Relayer Update client should return transaction")

			s.SolanaChain.SubmitChunkedUpdateClient(ctx, s.T(), s.Require(), updateResp, s.SolanaUser)

			// Now retrieve and relay the GMP packet
			resp, err := s.RelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
				SrcChain:    simd.Config().ChainID,
				DstChain:    testvalues.SolanaChainID,
				SourceTxIds: [][]byte{cosmosGMPTxHash},
				SrcClientId: CosmosClientID,
				DstClientId: SolanaClientID,
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Tx, "Relay should return transaction")

			// Execute on Solana using chunked submission
			solanaRelayTxSig, err = s.SolanaChain.SubmitChunkedRelayPackets(ctx, s.T(), resp, s.SolanaUser)
			s.Require().NoError(err)
			s.T().Logf("%s: GMP execution completed on Solana", userLabel)

			return solanaRelayTxSig
		}

		s.T().Logf("Setup complete - User0 key: %s, User1 key: %s", s.CosmosUsers[0].FormattedAddress(), s.CosmosUsers[1].FormattedAddress())
	}))

	// Check initial counter states
	var initialCounterUser0, initialCounterUser1 uint64
	s.Require().True(s.Run("Check Initial Counter States", func() {
		initialCounterUser0 = getCounterValue(s.CosmosUsers[0].FormattedAddress())
		initialCounterUser1 = getCounterValue(s.CosmosUsers[1].FormattedAddress())
		s.T().Logf("Initial counter for user0: %d", initialCounterUser0)
		s.T().Logf("Initial counter for user1: %d", initialCounterUser1)
	}))

	// Send increment from User 0
	var cosmosGMPTxHashUser0 []byte
	s.Require().True(s.Run("User0: Send GMP increment call from Cosmos", func() {
		cosmosGMPTxHashUser0 = sendGMPIncrement(s.CosmosUsers[0], DefaultIncrementAmount)
		s.Require().NotEmpty(cosmosGMPTxHashUser0)
	}))

	// Relay User 0's increment
	var solanaRelayTxSigUser0 solanago.Signature
	s.Require().True(s.Run("User0: Relay and execute GMP packet on Solana", func() {
		solanaRelayTxSigUser0 = relayGMPPacket(cosmosGMPTxHashUser0, "User0")
	}))

	s.Require().True(s.Run("User0: Verify counter was incremented", func() {
		newCounter := getCounterValue(s.CosmosUsers[0].FormattedAddress())
		expectedCounter := initialCounterUser0 + DefaultIncrementAmount
		s.Require().Equal(expectedCounter, newCounter)
		s.T().Logf("User0: Counter successfully incremented from %d to %d", initialCounterUser0, newCounter)
	}))

	// Now send increment from User 1
	var cosmosGMPTxHashUser1 []byte
	s.Require().True(s.Run("User1: Send GMP increment call from Cosmos", func() {
		cosmosGMPTxHashUser1 = sendGMPIncrement(s.CosmosUsers[1], 3) // Increment by 3 for variety
		s.Require().NotEmpty(cosmosGMPTxHashUser1)
	}))

	// Relay User 1's increment
	var solanaRelayTxSigUser1 solanago.Signature
	s.Require().True(s.Run("User1: Relay and execute GMP packet on Solana", func() {
		solanaRelayTxSigUser1 = relayGMPPacket(cosmosGMPTxHashUser1, "User1")
	}))

	s.Require().True(s.Run("User1: Verify counter was incremented", func() {
		newCounter := getCounterValue(s.CosmosUsers[1].FormattedAddress())
		expectedCounter := initialCounterUser1 + 3 // We incremented by 3
		s.Require().Equal(expectedCounter, newCounter)
		s.T().Logf("User1: Counter successfully incremented from %d to %d", initialCounterUser1, newCounter)
	}))

	s.Require().True(s.Run("Verify final counter states for both users", func() {
		finalCounterUser0 := getCounterValue(s.CosmosUsers[0].FormattedAddress())
		finalCounterUser1 := getCounterValue(s.CosmosUsers[1].FormattedAddress())

		// User 0 should have: initial + DefaultIncrementAmount (5)
		expectedFinalUser0 := initialCounterUser0 + DefaultIncrementAmount
		s.Require().Equal(expectedFinalUser0, finalCounterUser0)

		// User 1 should have: initial + 3
		expectedFinalUser1 := initialCounterUser1 + 3
		s.Require().Equal(expectedFinalUser1, finalCounterUser1)

		s.T().Logf("Final counter states - User0: %d (expected: %d), User1: %d (expected: %d)",
			finalCounterUser0, expectedFinalUser0, finalCounterUser1, expectedFinalUser1)
	}))

	s.Require().True(s.Run("Relay acknowledgments back to Cosmos", func() {
		simd := s.CosmosChains[0]

		s.Require().True(s.Run("Relay User0 acknowledgment", func() {
			var ackRelayTxBodyBz []byte
			s.Require().True(s.Run("Retrieve acknowledgment relay tx", func() {
				resp, err := s.RelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
					SrcChain:    testvalues.SolanaChainID,
					DstChain:    simd.Config().ChainID,
					SourceTxIds: [][]byte{[]byte(solanaRelayTxSigUser0.String())},
					SrcClientId: SolanaClientID,
					DstClientId: CosmosClientID,
				})
				s.Require().NoError(err)
				s.Require().NotEmpty(resp.Tx)
				s.T().Logf("Retrieved User0 GMP acknowledgment relay transaction")

				ackRelayTxBodyBz = resp.Tx
			}))

			s.Require().True(s.Run("Broadcast acknowledgment on Cosmos", func() {
				relayTxResult := s.MustBroadcastSdkTxBody(ctx, simd, s.CosmosUsers[0], CosmosDefaultGasLimit, ackRelayTxBodyBz)
				s.T().Logf("User0 GMP acknowledgment relay transaction: %s (code: %d, gas: %d)",
					relayTxResult.TxHash, relayTxResult.Code, relayTxResult.GasUsed)
			}))
		}))

		s.Require().True(s.Run("Relay User1 acknowledgment", func() {
			var ackRelayTxBodyBz []byte
			s.Require().True(s.Run("Retrieve acknowledgment relay tx", func() {
				resp, err := s.RelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
					SrcChain:    testvalues.SolanaChainID,
					DstChain:    simd.Config().ChainID,
					SourceTxIds: [][]byte{[]byte(solanaRelayTxSigUser1.String())},
					SrcClientId: SolanaClientID,
					DstClientId: CosmosClientID,
				})
				s.Require().NoError(err)
				s.Require().NotEmpty(resp.Tx)
				s.T().Logf("Retrieved User1 GMP acknowledgment relay transaction")

				ackRelayTxBodyBz = resp.Tx
			}))

			s.Require().True(s.Run("Broadcast acknowledgment on Cosmos", func() {
				relayTxResult := s.MustBroadcastSdkTxBody(ctx, simd, s.CosmosUsers[0], CosmosDefaultGasLimit, ackRelayTxBodyBz)
				s.T().Logf("User1 GMP acknowledgment relay transaction: %s (code: %d, gas: %d)",
					relayTxResult.TxHash, relayTxResult.Code, relayTxResult.GasUsed)
			}))
		}))

		s.T().Logf("GMP calls from Cosmos successfully acknowledged")
	}))
}

// Test_GMPSPLTokenTransfer tests transferring SPL tokens via GMP from Cosmos to Solana
// This demonstrates the SPL token transfer example from the ADR where:
// 1. A Cosmos user controls an ICS27 Account PDA on Solana
// 2. The ICS27 PDA owns SPL token accounts
// 3. Through GMP, the Cosmos user sends cross-chain calls to transfer tokens
func (s *IbcEurekaSolanaTestSuite) Test_GMPSPLTokenTransferFromCosmos() {
	ctx := context.Background()

	s.UseMockWasmClient = true
	s.SetupGMP = true

	s.SetupSuite(ctx)

	simd := s.CosmosChains[0]
	cosmosUser := s.CosmosUsers[0]

	// Setup SPL token infrastructure
	var tokenMint solanago.PublicKey
	var ics27AccountPDA solanago.PublicKey
	var sourceTokenAccount solanago.PublicKey
	var destTokenAccount solanago.PublicKey
	var recipientWallet *solanago.Wallet

	s.Require().True(s.Run("Setup SPL Token Infrastructure", func() {
		s.Require().True(s.Run("Create Test SPL Token Mint", func() {
			var err error
			tokenMint, err = s.createSPLTokenMint(ctx, 6)
			s.Require().NoError(err)
			s.T().Logf("Created test SPL token mint: %s (6 decimals)", tokenMint.String())
		}))

		s.Require().True(s.Run("Derive ICS27 Account PDA", func() {
			ics27AccountPDA, _ = solana.GMPAccountPDA(CosmosClientID, cosmosUser.FormattedAddress(), []byte{})
			s.T().Logf("ICS27 Account PDA for Cosmos user: %s", ics27AccountPDA.String())
		}))

		s.Require().True(s.Run("Create Token Accounts", func() {
			var err error

			// Create source token account (owned by ICS27 PDA)
			sourceTokenAccount, err = s.createTokenAccount(ctx, tokenMint, ics27AccountPDA)
			s.Require().NoError(err)
			s.T().Logf("Created source token account (owned by ICS27 PDA): %s", sourceTokenAccount.String())

			// Create recipient wallet and destination token account
			recipientWallet, err = s.SolanaChain.CreateAndFundWallet()
			s.Require().NoError(err)

			destTokenAccount, err = s.createTokenAccount(ctx, tokenMint, recipientWallet.PublicKey())
			s.Require().NoError(err)
			s.T().Logf("Created destination token account (owned by recipient): %s", destTokenAccount.String())
		}))

		s.Require().True(s.Run("Mint Tokens to ICS27 PDA", func() {
			// Mint 10 tokens (10,000,000 with 6 decimals)
			mintAmount := SPLTokenMintAmount
			err := s.mintTokensTo(ctx, tokenMint, sourceTokenAccount, mintAmount)
			s.Require().NoError(err)

			balance, err := s.getTokenBalance(ctx, sourceTokenAccount)
			s.Require().NoError(err)
			s.Require().Equal(mintAmount, balance)
			s.T().Logf("Minted %d tokens to ICS27 PDA's token account", mintAmount)
		}))
	}))

	// Execute SPL token transfer via GMP
	var cosmosGMPTxHash []byte
	transferAmount := SPLTokenTransferAmount // 1 token (1,000,000 with 6 decimals)

	s.Require().True(s.Run("Send GMP SPL Token Transfer from Cosmos", func() {
		timeout := uint64(time.Now().Add(30 * time.Minute).Unix())

		// Build SPL transfer instruction
		splTransferInstruction := token.NewTransferInstruction(
			transferAmount,
			sourceTokenAccount,
			destTokenAccount,
			ics27AccountPDA, // Authority - will be signed by GMP program via invoke_signed
			[]solanago.PublicKey{},
		).Build()

		// Get instruction data
		instructionData, err := splTransferInstruction.Data()
		s.Require().NoError(err)

		// Create SolanaInstruction protobuf
		// Note: PayerPosition is left unset (nil) - NO payer injection since SPL Transfer doesn't create accounts
		// SPL Transfer requires exactly 3 accounts: source, destination, authority
		// The authority (ICS27 PDA) must be marked as PDA_SIGNER so GMP program builds CPI with it as signer
		solanaInstruction := &solanatypes.SolanaInstruction{
			ProgramId: token.ProgramID.Bytes(),
			Data:      instructionData,
			Accounts: []*solanatypes.SolanaAccountMeta{
				{Pubkey: sourceTokenAccount.Bytes(), IsSigner: false, IsWritable: true}, // [0] source
				{Pubkey: destTokenAccount.Bytes(), IsSigner: false, IsWritable: true},   // [1] destination
				{Pubkey: ics27AccountPDA.Bytes(), IsSigner: true, IsWritable: false},    // [2] authority (GMP PDA signs via invoke_signed)
			},
			// PayerPosition is nil - no payer injection needed
		}

		payload, err := proto.Marshal(solanaInstruction)
		s.Require().NoError(err)

		// Send GMP call
		resp, err := s.BroadcastMessages(ctx, simd, cosmosUser, 2_000_000, &gmptypes.MsgSendCall{
			SourceClient:     CosmosClientID,
			Sender:           cosmosUser.FormattedAddress(),
			Receiver:         token.ProgramID.String(),
			Salt:             []byte{},
			Payload:          payload,
			TimeoutTimestamp: timeout,
			Memo:             fmt.Sprintf("SPL token transfer: %d tokens", transferAmount),
			Encoding:         testvalues.Ics27ProtobufEncoding,
		})
		s.Require().NoError(err)

		cosmosGMPTxHashBytes, err := hex.DecodeString(resp.TxHash)
		s.Require().NoError(err)
		cosmosGMPTxHash = cosmosGMPTxHashBytes

		s.T().Logf("GMP SPL transfer packet sent from Cosmos: %s", resp.TxHash)
	}))

	// Record initial balances
	var initialSourceBalance uint64
	var initialDestBalance uint64
	s.Require().True(s.Run("Record Initial Token Balances", func() {
		var err error
		initialSourceBalance, err = s.getTokenBalance(ctx, sourceTokenAccount)
		s.Require().NoError(err)

		initialDestBalance, err = s.getTokenBalance(ctx, destTokenAccount)
		s.Require().NoError(err)

		s.T().Logf("Initial balances - Source: %d, Dest: %d", initialSourceBalance, initialDestBalance)
	}))

	// Relay and execute on Solana
	var solanaRelayTxSig solanago.Signature
	s.Require().True(s.Run("Relay and Execute SPL Transfer on Solana", func() {
		s.Require().True(s.Run("Update Tendermint client on Solana", func() {
			updateResp, err := s.RelayerClient.UpdateClient(context.Background(), &relayertypes.UpdateClientRequest{
				SrcChain:    simd.Config().ChainID,
				DstChain:    testvalues.SolanaChainID,
				DstClientId: SolanaClientID,
			})
			s.Require().NoError(err, "Relayer Update Client failed")
			s.Require().NotEmpty(updateResp.Tx, "Relayer Update client should return transaction")

			s.SolanaChain.SubmitChunkedUpdateClient(ctx, s.T(), s.Require(), updateResp, s.SolanaUser)
		}))

		s.Require().True(s.Run("Retrieve relay tx from relayer", func() {
			resp, err := s.RelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
				SrcChain:    simd.Config().ChainID,
				DstChain:    testvalues.SolanaChainID,
				SourceTxIds: [][]byte{cosmosGMPTxHash},
				SrcClientId: CosmosClientID,
				DstClientId: SolanaClientID,
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Tx, "Relay should return transaction")

			solanaRelayTxSig, err = s.SolanaChain.SubmitChunkedRelayPackets(ctx, s.T(), resp, s.SolanaUser)
			s.Require().NoError(err)
			s.T().Logf("SPL transfer executed on Solana: %s", solanaRelayTxSig)
		}))
	}))

	// Verify transfer completed
	s.Require().True(s.Run("Verify SPL Token Transfer", func() {
		finalSourceBalance, err := s.getTokenBalance(ctx, sourceTokenAccount)
		s.Require().NoError(err)

		finalDestBalance, err := s.getTokenBalance(ctx, destTokenAccount)
		s.Require().NoError(err)

		expectedSourceBalance := initialSourceBalance - transferAmount
		expectedDestBalance := initialDestBalance + transferAmount

		s.Require().Equal(expectedSourceBalance, finalSourceBalance,
			"Source balance should decrease by transfer amount")
		s.Require().Equal(expectedDestBalance, finalDestBalance,
			"Destination balance should increase by transfer amount")

		s.T().Logf("Transfer verified!")
		s.T().Logf("  Source: %d → %d (-%d)", initialSourceBalance, finalSourceBalance, transferAmount)
		s.T().Logf("  Dest:   %d → %d (+%d)", initialDestBalance, finalDestBalance, transferAmount)
	}))

	// Relay acknowledgment back to Cosmos
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
			s.T().Logf("Retrieved acknowledgment relay transaction")
		}))

		s.Require().True(s.Run("Broadcast acknowledgment on Cosmos", func() {
			relayTxResult := s.MustBroadcastSdkTxBody(ctx, simd, cosmosUser, CosmosDefaultGasLimit, ackRelayTxBodyBz)
			s.T().Logf("SPL transfer acknowledgment relay transaction: %s (code: %d, gas: %d)",
				relayTxResult.TxHash, relayTxResult.Code, relayTxResult.GasUsed)
		}))

		s.T().Logf("✓ SPL token transfer via GMP completed successfully")
		s.T().Logf("  Cosmos user %s controlled Solana ICS27 PDA %s",
			cosmosUser.FormattedAddress(), ics27AccountPDA.String())
		s.T().Logf("  Transferred %d tokens from ICS27 PDA to recipient", transferAmount)
	}))
}

// SPL Token Helper Functions

// createSPLTokenMint creates a new SPL token mint with specified decimals
func (s *IbcEurekaSolanaTestSuite) createSPLTokenMint(ctx context.Context, decimals uint8) (solanago.PublicKey, error) {
	mintAccount := solanago.NewWallet()
	mintPubkey := mintAccount.PublicKey()

	// Get minimum balance for rent exemption (mint account is 82 bytes)
	const mintAccountSize = uint64(82)
	rentExemption, err := s.SolanaChain.RPCClient.GetMinimumBalanceForRentExemption(ctx, mintAccountSize, "confirmed")
	if err != nil {
		return solanago.PublicKey{}, err
	}

	// Create mint account
	createAccountIx := system.NewCreateAccountInstruction(
		rentExemption,
		mintAccountSize,
		token.ProgramID,
		s.SolanaUser.PublicKey(),
		mintPubkey,
	).Build()

	// Initialize mint
	initMintIx := token.NewInitializeMint2Instruction(
		decimals,
		s.SolanaUser.PublicKey(), // Mint authority
		s.SolanaUser.PublicKey(), // Freeze authority
		mintPubkey,
	).Build()

	// Build transaction using the chain helper
	tx, err := s.SolanaChain.NewTransactionFromInstructions(
		s.SolanaUser.PublicKey(),
		createAccountIx,
		initMintIx,
	)
	if err != nil {
		return solanago.PublicKey{}, err
	}

	// Sign and broadcast with both payer and mint account (with retry)
	_, err = s.SolanaChain.SignAndBroadcastTxWithRetry(ctx, tx, s.SolanaUser, mintAccount)
	if err != nil {
		return solanago.PublicKey{}, err
	}

	return mintPubkey, nil
}

// createTokenAccount creates a new SPL token account for the specified owner
func (s *IbcEurekaSolanaTestSuite) createTokenAccount(ctx context.Context, mint, owner solanago.PublicKey) (solanago.PublicKey, error) {
	tokenAccount := solanago.NewWallet()
	tokenAccountPubkey := tokenAccount.PublicKey()

	// Token account size is 165 bytes
	const tokenAccountSize = uint64(165)

	// Calculate rent exemption
	rentExemption, err := s.SolanaChain.RPCClient.GetMinimumBalanceForRentExemption(ctx, tokenAccountSize, "confirmed")
	if err != nil {
		return solanago.PublicKey{}, err
	}

	// Create account instruction
	createAccountIx := system.NewCreateAccountInstruction(
		rentExemption,
		tokenAccountSize,
		token.ProgramID,
		s.SolanaUser.PublicKey(),
		tokenAccountPubkey,
	).Build()

	// Initialize token account (using InitializeAccount3 which doesn't require rent sysvar)
	// Parameters: owner, account, mint
	initAccountIx := token.NewInitializeAccount3Instruction(
		owner,
		tokenAccountPubkey,
		mint,
	).Build()

	// Build transaction using the chain helper
	tx, err := s.SolanaChain.NewTransactionFromInstructions(
		s.SolanaUser.PublicKey(),
		createAccountIx,
		initAccountIx,
	)
	if err != nil {
		return solanago.PublicKey{}, err
	}

	// Sign and broadcast with both payer and token account (with retry)
	_, err = s.SolanaChain.SignAndBroadcastTxWithRetry(ctx, tx, s.SolanaUser, tokenAccount)
	if err != nil {
		return solanago.PublicKey{}, err
	}

	return tokenAccountPubkey, nil
}

// mintTokensTo mints tokens to a specified token account
func (s *IbcEurekaSolanaTestSuite) mintTokensTo(ctx context.Context, mint, destination solanago.PublicKey, amount uint64) error {
	mintToIx := token.NewMintToInstruction(
		amount,
		mint,
		destination,
		s.SolanaUser.PublicKey(), // Mint authority
		[]solanago.PublicKey{},
	).Build()

	tx, err := s.SolanaChain.NewTransactionFromInstructions(
		s.SolanaUser.PublicKey(),
		mintToIx,
	)
	if err != nil {
		return err
	}

	_, err = s.SolanaChain.SignAndBroadcastTxWithRetry(ctx, tx, s.SolanaUser)
	return err
}

// getTokenBalance retrieves the token balance for a token account
func (s *IbcEurekaSolanaTestSuite) getTokenBalance(ctx context.Context, tokenAccount solanago.PublicKey) (uint64, error) {
	accountInfo, err := s.SolanaChain.RPCClient.GetAccountInfo(ctx, tokenAccount)
	if err != nil {
		return 0, err
	}

	if accountInfo.Value == nil {
		return 0, fmt.Errorf("token account not found")
	}

	data := accountInfo.Value.Data.GetBinary()
	if len(data) < 72 {
		return 0, fmt.Errorf("invalid token account data")
	}

	// Token balance is at offset 64 (8 bytes, little endian)
	balance := binary.LittleEndian.Uint64(data[64:72])
	return balance, nil
}

// AccountState represents the ICS27 GMP account state PDA
// This mirrors the Rust struct in programs/solana/programs/ics27-gmp/src/state.rs
type AccountState struct {
	ClientID       string
	Sender         string
	Salt           []byte
	Nonce          uint64
	CreatedAt      int64
	LastExecutedAt int64
	ExecutionCount uint64
	Bump           uint8
}

// getICS27AccountNonce retrieves the nonce from an ICS27 account state PDA
// Returns 0 if account doesn't exist or cannot be read
func (s *IbcEurekaSolanaTestSuite) getICS27AccountNonce(ctx context.Context, accountPDA solanago.PublicKey) uint64 {
	accountInfo, err := s.SolanaChain.RPCClient.GetAccountInfo(ctx, accountPDA)
	if err != nil || accountInfo.Value == nil {
		return 0 // Account doesn't exist yet
	}

	data := accountInfo.Value.Data.GetBinary()
	if len(data) < 8 {
		return 0
	}

	// Create Borsh decoder and skip discriminator
	decoder := bin.NewBorshDecoder(data)
	_, err = decoder.ReadDiscriminator()
	if err != nil {
		return 0
	}

	// Decode AccountState using Borsh
	var accountState AccountState
	err = decoder.Decode(&accountState.ClientID)
	if err != nil {
		return 0
	}
	err = decoder.Decode(&accountState.Sender)
	if err != nil {
		return 0
	}
	err = decoder.Decode(&accountState.Salt)
	if err != nil {
		return 0
	}
	err = decoder.Decode(&accountState.Nonce)
	if err != nil {
		return 0
	}

	return accountState.Nonce
}

func (s *IbcEurekaSolanaTestSuite) Test_GMPSendCallFromSolana() {
	ctx := context.Background()

	s.UseMockWasmClient = true
	s.SetupGMP = true

	s.SetupSuite(ctx)

	simd := s.CosmosChains[0]

	testAmount := sdk.NewCoins(sdk.NewCoin(simd.Config().Denom, sdkmath.NewInt(CosmosTestAmount)))
	testCosmosUser := s.CreateAndFundCosmosUserWithBalance(ctx, simd, testAmount[0].Amount.Int64())

	var computedAddress sdk.AccAddress
	s.Require().True(s.Run("Fund pre-computed ICS27 address on Cosmos", func() {
		solanaUserAddress := s.SolanaUser.PublicKey().String()

		// Use CosmosClientID (08-wasm-0) - the dest_client on Cosmos
		// The GMP keeper creates accounts using NewAccountIdentifier(destClient, sender, salt)
		res, err := e2esuite.GRPCQuery[gmptypes.QueryAccountAddressResponse](ctx, simd, &gmptypes.QueryAccountAddressRequest{
			ClientId: CosmosClientID,
			Sender:   solanaUserAddress,
			Salt:     "",
		})
		s.Require().NoError(err)
		s.Require().NotEmpty(res.AccountAddress)

		computedAddress, err = sdk.AccAddressFromBech32(res.AccountAddress)
		s.Require().NoError(err)

		s.T().Logf("ICS27 account on Cosmos: %s", computedAddress.String())

		_, err = s.BroadcastMessages(ctx, simd, testCosmosUser, CosmosDefaultGasLimit, &banktypes.MsgSend{
			FromAddress: testCosmosUser.FormattedAddress(),
			ToAddress:   computedAddress.String(),
			Amount:      testAmount,
		})
		s.Require().NoError(err)
	}))

	s.Require().True(s.Run("Verify initial balance on Cosmos", func() {
		resp, err := e2esuite.GRPCQuery[banktypes.QueryBalanceResponse](ctx, simd, &banktypes.QueryBalanceRequest{
			Address: computedAddress.String(),
			Denom:   simd.Config().Denom,
		})
		s.Require().NoError(err)
		s.Require().NotNil(resp.Balance)
		s.Require().Equal(testAmount[0].Amount.Int64(), resp.Balance.Amount.Int64())
	}))

	var solanaPacketTxHash string
	s.Require().True(s.Run("Send call from Solana", func() {
		timeout := uint64(time.Now().Add(30 * time.Minute).Unix())

		var payload []byte
		s.Require().True(s.Run("Prepare GMP payload", func() {
			msgSend := &banktypes.MsgSend{
				FromAddress: computedAddress.String(),
				ToAddress:   testCosmosUser.FormattedAddress(),
				Amount:      testAmount,
			}

			var err error
			payload, err = gmphelpers.NewPayload_FromProto([]proto.Message{msgSend})
			s.Require().NoError(err)
			s.T().Logf("Encoded GMP payload (%d bytes)", len(payload))
		}))

		var gmpAppStatePDA, routerStatePDA, routerCallerPDA, clientPDA, ibcAppPDA, clientSequencePDA solanago.PublicKey
		s.Require().True(s.Run("Derive required PDAs", func() {
			gmpAppStatePDA, _ = solana.GMPAppStatePDA(GMPPortID)
			routerStatePDA, _ = solana.RouterStatePDA()
			routerCallerPDA, _ = solana.GMPRouterCallerPDA()
			clientPDA, _ = solana.RouterClientPDA(SolanaClientID)
			ibcAppPDA, _ = solana.RouterIBCAppPDA(GMPPortID)
			clientSequencePDA, _ = solana.RouterClientSequencePDA(SolanaClientID)

			s.T().Logf("Derived PDAs: gmpAppState=%s, routerState=%s, client=%s",
				gmpAppStatePDA.String(), routerStatePDA.String(), clientPDA.String())
		}))

		var packetCommitmentPDA solanago.PublicKey
		var nextSequence uint64
		s.Require().True(s.Run("Get next sequence number and packet commitment PDA", func() {
			var err error
			nextSequence, err = s.SolanaChain.GetNextSequenceNumber(ctx, clientSequencePDA)
			s.Require().NoError(err)

			packetCommitmentPDA, _ = solana.RouterPacketCommitmentPDA(SolanaClientID, nextSequence)
			s.T().Logf("Using sequence number: %d", nextSequence)
		}))

		var sendCallInstruction solanago.Instruction
		s.Require().True(s.Run("Build send_call instruction", func() {
			var err error
			sendCallInstruction, err = ics27_gmp.NewSendCallInstruction(
				ics27_gmp.SendCallMsg{
					SourceClient:     SolanaClientID,
					TimeoutTimestamp: int64(timeout),
					Receiver:         solanago.PublicKey{},
					Salt:             []byte{},
					Payload:          payload,
					Memo:             "send from Solana to Cosmos",
				},
				gmpAppStatePDA,
				s.SolanaUser.PublicKey(),
				s.SolanaUser.PublicKey(),
				ics26_router.ProgramID,
				routerStatePDA,
				clientSequencePDA,
				packetCommitmentPDA,
				routerCallerPDA,
				ibcAppPDA,
				clientPDA,
				solanago.SystemProgramID,
			)
			s.Require().NoError(err)
			s.T().Log("Built send_call instruction")
		}))

		s.Require().True(s.Run("Broadcast transaction", func() {
			tx, err := s.SolanaChain.NewTransactionFromInstructions(
				s.SolanaUser.PublicKey(),
				sendCallInstruction,
			)
			s.Require().NoError(err)

			sig, err := s.SolanaChain.SignAndBroadcastTxWithRetry(ctx, tx, s.SolanaUser)
			s.Require().NoError(err)
			s.Require().NotEmpty(sig)

			solanaPacketTxHash = sig.String()
			s.T().Logf("Send call transaction: %s", solanaPacketTxHash)
		}))
	}))

	var ackTxHash []byte
	s.Require().True(s.Run("Receive packet in Cosmos", func() {
		var recvRelayTx []byte
		s.Require().True(s.Run("Retrieve relay tx", func() {
			txHashBytes := []byte(solanaPacketTxHash)

			resp, err := s.RelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
				SrcChain:    testvalues.SolanaChainID,
				DstChain:    simd.Config().ChainID,
				SourceTxIds: [][]byte{txHashBytes},
				SrcClientId: SolanaClientID,
				DstClientId: CosmosClientID,
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Tx)
			s.Require().Empty(resp.Address)

			recvRelayTx = resp.Tx
		}))

		s.Require().True(s.Run("Submit relay tx to Cosmos", func() {
			receipt := s.MustBroadcastSdkTxBody(ctx, simd, s.CosmosUsers[0], 2_000_000, recvRelayTx)
			s.T().Logf("Recv packet tx result: code=%d, log=%s, gas=%d", receipt.Code, receipt.RawLog, receipt.GasUsed)

			s.Require().Equal(uint32(0), receipt.Code, "Tx should succeed")
			s.Require().NotEmpty(receipt.TxHash)

			var err error
			ackTxHash, err = hex.DecodeString(receipt.TxHash)
			s.Require().NoError(err)
		}))

		s.Require().True(s.Run("Verify balance changed on Cosmos", func() {
			resp, err := e2esuite.GRPCQuery[banktypes.QueryBalanceResponse](ctx, simd, &banktypes.QueryBalanceRequest{
				Address: computedAddress.String(),
				Denom:   simd.Config().Denom,
			})
			s.Require().NoError(err)
			s.Require().NotNil(resp.Balance)
			s.Require().Zero(resp.Balance.Amount.Int64())

			resp, err = e2esuite.GRPCQuery[banktypes.QueryBalanceResponse](ctx, simd, &banktypes.QueryBalanceRequest{
				Address: testCosmosUser.FormattedAddress(),
				Denom:   simd.Config().Denom,
			})
			s.Require().NoError(err)
			s.Require().NotNil(resp.Balance)
			s.Require().Equal(testAmount[0].Amount.Int64(), resp.Balance.Amount.Int64())
		}))
	}))

	s.Require().True(s.Run("Acknowledge packet in Solana", func() {
		s.Require().True(s.Run("Update Tendermint client on Solana", func() {
			resp, err := s.RelayerClient.UpdateClient(context.Background(), &relayertypes.UpdateClientRequest{
				SrcChain:    simd.Config().ChainID,
				DstChain:    testvalues.SolanaChainID,
				DstClientId: SolanaClientID,
			})
			s.Require().NoError(err, "Relayer Update Client failed")
			s.Require().NotEmpty(resp.Tx, "Relayer Update client should return transaction")

			s.SolanaChain.SubmitChunkedUpdateClient(ctx, s.T(), s.Require(), resp, s.SolanaUser)
		}))

		s.Require().True(s.Run("Relay acknowledgement", func() {
			resp, err := s.RelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
				SrcChain:    simd.Config().ChainID,
				DstChain:    testvalues.SolanaChainID,
				SourceTxIds: [][]byte{ackTxHash},
				SrcClientId: CosmosClientID,
				DstClientId: SolanaClientID,
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Tx, "Relay should return transaction")

			sig, err := s.SolanaChain.SubmitChunkedRelayPackets(ctx, s.T(), resp, s.SolanaUser)
			s.Require().NoError(err)
			s.T().Logf("Acknowledgement transaction broadcasted: %s", sig)
		}))

		s.Require().True(s.Run("Verify acknowledgement was processed", func() {
			s.SolanaChain.VerifyPacketCommitmentDeleted(ctx, s.T(), s.Require(), SolanaClientID, 1)
		}))
	}))
}

// Test_GMPTimeoutFromSolana tests GMP packet timeout when sent from Solana to Cosmos
//
// HIGH-LEVEL FLOW:
//
// 1. Setup Phase
//   - Deploy ICS27 GMP program on Solana
//   - Fund ICS27 account on Cosmos with test tokens
//
// 2. Send Packet (Solana → Cosmos)
//   - Solana sends IBC packet via send_call instruction
//   - Packet type: GMP call packet (ICS27 general message passing)
//   - Payload: Protobuf-encoded MsgSend (bank transfer from ICS27 account to test user)
//   - Timeout: 35 seconds from current time
//   - Packet commitment created on Solana (stores hash of packet data)
//
// 3. Retrieve Recv Transaction Before Timeout
//   - Retrieve the recv relay transaction from relayer (before timeout expires)
//   - This transaction will be used later to verify it fails after timeout
//
// 4. Timeout Expiry
//   - Packet expires on Cosmos (not processed in time)
//   - Wait 40 seconds for timeout to occur
//
// 5. Update Light Client
//
//   - Update Tendermint client on Solana
//
//   - Ensures client can verify latest Cosmos state for timeout proof
//
//     6. Relay Timeout (Cosmos → Solana)
//     NOTE: This test provides solanaPacketTxHash via TimeoutTxIds for explicit control.
//     In production, the relayer discovers timeouts automatically by:
//
//   - Monitoring SendPacket events and tracking pending packets
//
//   - Detecting when current_time > timeout_timestamp for unacknowledged packets
//
//   - Initiating timeout relay for expired packets
//
//     Relayer timeout workflow:
//     a) Query original send transaction on Solana using solanaPacketTxHash
//     b) Extract SendPacket event (sequence, payload, timeout, client IDs)
//     c) Query Cosmos state tree at path: ["ibc", destClient + 0x02 + sequence]
//     → Receipt does not exist in state (packet was never received)
//     (Path format: destination client ID + 0x02 (receipt discriminator) + sequence as big-endian u64)
//     (Note: 0x01=commitment, 0x02=receipt, 0x03=acknowledgement)
//     d) Determine timeout condition: current_time > timeout_timestamp AND receipt not found
//     e) Build absence proof (non-membership proof) showing packet receipt doesn't exist at trusted height
//     f) Return chunked Solana transactions:
//
//   - Membership proof chunks (proving Cosmos state tree at height H)
//
//   - Final on_timeout_packet instruction with assembled absence proof
//
//     7. Process Timeout on Solana
//     On-chain verification via on_timeout_packet instruction:
//     a) ICS26 router verifies timeout proof against Tendermint consensus state
//     b) Validate proof_height <= latest_client_height
//     c) Confirm current_time > packet.timeout_timestamp
//     d) Verify packet commitment exists and matches stored hash
//     e) Delete packet commitment from Solana state (cleanup)
//     f) Call ICS27 GMP app's on_timeout_packet via CPI
//     g) App performs application-specific timeout handling
//
// 8. Verify Timeout Effects
//   - Packet commitment deleted on Solana
//   - ICS27 account balance on Cosmos unchanged (MsgSend never executed)
//
// 9. Verify RecvPacket Fails After Timeout
//   - Attempt to broadcast the recv transaction that was retrieved before timeout
//   - Transaction should fail on Cosmos (packet already timed out)
func (s *IbcEurekaSolanaTestSuite) Test_GMPTimeoutFromSolana() {
	ctx := context.Background()

	s.UseMockWasmClient = true
	s.SetupGMP = true

	s.SetupSuite(ctx)

	simd := s.CosmosChains[0]

	testAmount := sdk.NewCoins(sdk.NewCoin(simd.Config().Denom, sdkmath.NewInt(CosmosTestAmount)))
	testCosmosUser := s.CreateAndFundCosmosUserWithBalance(ctx, simd, testAmount[0].Amount.Int64())

	var computedAddress sdk.AccAddress
	s.Require().True(s.Run("Fund pre-computed ICS27 address on Cosmos", func() {
		solanaUserAddress := s.SolanaUser.PublicKey().String()

		res, err := e2esuite.GRPCQuery[gmptypes.QueryAccountAddressResponse](ctx, simd, &gmptypes.QueryAccountAddressRequest{
			ClientId: CosmosClientID,
			Sender:   solanaUserAddress,
			Salt:     "",
		})
		s.Require().NoError(err)
		s.Require().NotEmpty(res.AccountAddress)

		computedAddress, err = sdk.AccAddressFromBech32(res.AccountAddress)
		s.Require().NoError(err)

		s.T().Logf("ICS27 account on Cosmos: %s", computedAddress.String())

		_, err = s.BroadcastMessages(ctx, simd, testCosmosUser, CosmosDefaultGasLimit, &banktypes.MsgSend{
			FromAddress: testCosmosUser.FormattedAddress(),
			ToAddress:   computedAddress.String(),
			Amount:      testAmount,
		})
		s.Require().NoError(err)
	}))

	var solanaPacketTxHash []byte
	var nextSequence uint64

	s.Require().True(s.Run("Send call from Solana with short timeout", func() {
		var payload []byte
		s.Require().True(s.Run("Prepare GMP payload", func() {
			msgSend := &banktypes.MsgSend{
				FromAddress: computedAddress.String(),
				ToAddress:   testCosmosUser.FormattedAddress(),
				Amount:      testAmount,
			}

			var err error
			payload, err = gmphelpers.NewPayload_FromProto([]proto.Message{msgSend})
			s.Require().NoError(err)
			s.T().Logf("Encoded GMP payload (%d bytes)", len(payload))
		}))

		var gmpAppStatePDA, routerStatePDA, routerCallerPDA, clientPDA, ibcAppPDA, clientSequencePDA solanago.PublicKey
		s.Require().True(s.Run("Derive required PDAs", func() {
			gmpAppStatePDA, _ = solana.GMPAppStatePDA(GMPPortID)
			routerStatePDA, _ = solana.RouterStatePDA()
			routerCallerPDA, _ = solana.GMPRouterCallerPDA()
			clientPDA, _ = solana.RouterClientPDA(SolanaClientID)
			ibcAppPDA, _ = solana.RouterIBCAppPDA(GMPPortID)
			clientSequencePDA, _ = solana.RouterClientSequencePDA(SolanaClientID)

			s.T().Logf("Derived PDAs: gmpAppState=%s, routerState=%s, client=%s",
				gmpAppStatePDA.String(), routerStatePDA.String(), clientPDA.String())
		}))

		var packetCommitmentPDA solanago.PublicKey
		s.Require().True(s.Run("Get next sequence number and packet commitment PDA", func() {
			var err error
			nextSequence, err = s.SolanaChain.GetNextSequenceNumber(ctx, clientSequencePDA)
			s.Require().NoError(err)

			packetCommitmentPDA, _ = solana.RouterPacketCommitmentPDA(SolanaClientID, nextSequence)
			s.T().Logf("Using sequence number: %d (timeout test)", nextSequence)
		}))

		var sendCallInstruction solanago.Instruction
		s.Require().True(s.Run("Build send_call instruction", func() {
			solanaClockTime, err := s.SolanaChain.GetSolanaClockTime(ctx)
			s.Require().NoError(err)

			// Using 35 seconds to provide buffer above the transaction execution delay
			timeout := uint64(solanaClockTime + 35)

			s.T().Logf("Setting timeout to: %d (solana_clock=%d + 35 seconds)", timeout, solanaClockTime)

			sendCallInstruction, err = ics27_gmp.NewSendCallInstruction(
				ics27_gmp.SendCallMsg{
					SourceClient:     SolanaClientID,
					TimeoutTimestamp: int64(timeout),
					Receiver:         solanago.PublicKey{},
					Salt:             []byte{},
					Payload:          payload,
					Memo:             "timeout test from Solana",
				},
				gmpAppStatePDA,
				s.SolanaUser.PublicKey(),
				s.SolanaUser.PublicKey(),
				ics26_router.ProgramID,
				routerStatePDA,
				clientSequencePDA,
				packetCommitmentPDA,
				routerCallerPDA,
				ibcAppPDA,
				clientPDA,
				solanago.SystemProgramID,
			)
			s.Require().NoError(err)
			s.T().Log("Built send_call instruction with short timeout")
		}))

		s.Require().True(s.Run("Broadcast transaction", func() {
			tx, err := s.SolanaChain.NewTransactionFromInstructions(
				s.SolanaUser.PublicKey(),
				sendCallInstruction,
			)
			s.Require().NoError(err)

			sig, err := s.SolanaChain.SignAndBroadcastTxWithRetry(ctx, tx, s.SolanaUser)
			s.Require().NoError(err)
			s.Require().NotEmpty(sig)

			solanaPacketTxHash = []byte(sig.String())
			s.T().Logf("Send call transaction (will timeout): %s", sig)
		}))
	}))

	// Retrieve the recv relay tx before timeout - we'll try to use it after timeout
	var recvRelayTxBodyBz []byte
	s.Require().True(s.Run("Retrieve recv relay tx before timeout", func() {
		resp, err := s.RelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
			SrcChain:    testvalues.SolanaChainID,
			DstChain:    simd.Config().ChainID,
			SourceTxIds: [][]byte{solanaPacketTxHash},
			SrcClientId: SolanaClientID,
			DstClientId: CosmosClientID,
		})
		s.Require().NoError(err)
		recvRelayTxBodyBz = resp.Tx
		s.T().Log("Retrieved recv relay transaction before timeout")
	}))

	// Sleep for 40 seconds to let the packet timeout (timeout is set to solana_time + 35 seconds)
	s.T().Log("Sleeping 40 seconds to let packet timeout...")
	time.Sleep(40 * time.Second)

	s.Require().True(s.Run("Relay timeout back to Solana", func() {
		// Update Tendermint client on Solana before relaying timeout
		// The relayer now queries Cosmos for current height for timeout proofs,
		// so we just need to ensure Solana has a recent consensus state
		s.Require().True(s.Run("Update Tendermint client on Solana", func() {
			resp, err := s.RelayerClient.UpdateClient(context.Background(), &relayertypes.UpdateClientRequest{
				SrcChain:    simd.Config().ChainID,
				DstChain:    testvalues.SolanaChainID,
				DstClientId: SolanaClientID,
			})
			s.Require().NoError(err, "Relayer Update Client failed")
			s.Require().NotEmpty(resp.Tx, "Relayer Update client should return transaction")

			s.SolanaChain.SubmitChunkedUpdateClient(ctx, s.T(), s.Require(), resp, s.SolanaUser)
		}))

		s.Require().True(s.Run("Relay timeout transaction", func() {
			resp, err := s.RelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
				SrcChain:     simd.Config().ChainID,
				DstChain:     testvalues.SolanaChainID,
				TimeoutTxIds: [][]byte{solanaPacketTxHash},
				SrcClientId:  CosmosClientID,
				DstClientId:  SolanaClientID,
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Tx, "Relay should return transaction")

			sig, err := s.SolanaChain.SubmitChunkedRelayPackets(ctx, s.T(), resp, s.SolanaUser)
			s.Require().NoError(err)
			s.T().Logf("Timeout transaction broadcasted: %s", sig)

			s.T().Log("Timeout successfully processed on Solana")
		}))

		s.Require().True(s.Run("Verify timeout effects", func() {
			s.Require().True(s.Run("Verify packet commitment deleted on Solana", func() {
				s.SolanaChain.VerifyPacketCommitmentDeleted(ctx, s.T(), s.Require(), SolanaClientID, nextSequence)
				s.T().Logf("Packet commitment successfully deleted from Solana for sequence %d", nextSequence)
			}))

			s.Require().True(s.Run("Verify Cosmos account balance unchanged", func() {
				// The MsgSend from ICS27 account never executed, so balance should remain the same
				balanceResp, err := e2esuite.GRPCQuery[banktypes.QueryBalanceResponse](ctx, simd, &banktypes.QueryBalanceRequest{
					Address: computedAddress.String(),
					Denom:   simd.Config().Denom,
				})
				s.Require().NoError(err)
				s.Require().Equal(testAmount[0].Amount.String(), balanceResp.Balance.Amount.String(),
					"ICS27 account balance should remain unchanged after timeout")
				s.T().Logf("ICS27 account balance: %s (unchanged)", balanceResp.Balance.Amount.String())
			}))

			s.Require().True(s.Run("Verify recvPacket fails on Cosmos after timeout", func() {
				_, err := s.BroadcastSdkTxBody(ctx, simd, s.CosmosUsers[0], 2_000_000, recvRelayTxBodyBz)
				s.Require().Error(err)
			}))
		}))
	}))
}

// Test_GMPTimeoutFromCosmos tests GMP packet timeout when sent from Cosmos to Solana
//
// HIGH-LEVEL FLOW:
//
// 1. Setup Phase
//   - Create SPL token infrastructure on Solana
//   - Set up mint, ICS27 account PDA, and token accounts
//   - Fund source token account with 1M tokens (6 decimals)
//
// 2. Send Packet (Cosmos → Solana)
//   - Cosmos sends IBC packet via MsgSendCall
//   - Packet type: GMP call packet (ICS27 general message passing)
//   - Payload: Protobuf-encoded SolanaInstruction (SPL token transfer: 1M tokens from ICS27 account to recipient)
//   - Timeout: 35 seconds from current time
//   - Packet commitment created on Cosmos (stores hash of packet data)
//
// 3. Retrieve Recv Transactions Before Timeout
//   - Retrieve the recv relay transactions from relayer (before timeout expires)
//   - These transactions will be used later to verify they fail after timeout
//
// 4. Timeout Expiry
//
//   - Packet expires on Solana (not relayed/processed in time)
//
//   - Wait 40 seconds for timeout to occur
//
//     5. Relay Timeout (Solana → Cosmos)
//     NOTE: This test provides cosmosGMPTxHash via TimeoutTxIds for explicit control.
//     In production, the relayer discovers timeouts automatically by:
//
//   - Monitoring SendPacket events and tracking pending packets
//
//   - Detecting when current_time > timeout_timestamp for unacknowledged packets
//
//   - Initiating timeout relay for expired packets
//
//     Relayer timeout workflow:
//     a) Query original send transaction on Cosmos using cosmosGMPTxHash
//     b) Extract SendPacket event (sequence, payload, timeout, client IDs)
//     c) Query Solana state tree at path: ["ibc", destClient + 0x02 + sequence]
//     → Receipt does not exist in state (packet was never received)
//     (Path format: destination client ID + 0x02 (receipt discriminator) + sequence as big-endian u64)
//     (Note: 0x01=commitment, 0x02=receipt, 0x03=acknowledgement)
//     d) Determine timeout condition: current_time > timeout_timestamp AND receipt not found
//     e) Build absence proof (non-membership proof) showing packet receipt doesn't exist at trusted height
//     f) Update Wasm light client if needed for proof verification
//     g) Return single Cosmos transaction (MsgTimeout) with absence proof
//
//     6. Process Timeout on Cosmos
//     On-chain verification via MsgTimeout transaction:
//     a) x/gmp module receives MsgTimeout transaction
//     b) Wasm light client verifies absence proof against Solana consensus state
//     c) Validate proof_height <= latest_client_height
//     d) Confirm current_time > packet.timeout_timestamp
//     e) Verify packet commitment exists and matches hash(packet_data)
//     f) Delete packet commitment from Cosmos state (cleanup)
//     g) Call application's OnTimeoutPacket callback
//     h) App performs application-specific timeout handling (refunds, state reversion)
//     i) Emit TimeoutPacket event
//
// 7. Verify Timeout Effects
//   - Packet commitment deleted on Cosmos
//   - Source SPL token account still has all 1M tokens (transfer never executed)
//   - Destination SPL token account has 0 tokens (never received)
//
// 8. Verify RecvPacket Fails After Timeout
//   - Attempt to broadcast the recv transactions that were retrieved before timeout
//   - Transactions should fail on Solana (packet already timed out)
func (s *IbcEurekaSolanaTestSuite) Test_GMPTimeoutFromCosmos() {
	ctx := context.Background()

	s.UseMockWasmClient = true
	s.SetupGMP = true

	s.SetupSuite(ctx)

	simd := s.CosmosChains[0]
	cosmosUser := s.CosmosUsers[0]

	// Create SPL token and accounts
	var tokenMint solanago.PublicKey
	var ics27AccountPDA solanago.PublicKey
	var sourceTokenAccount solanago.PublicKey

	const tokenAmount = uint64(1_000_000) // 1 token with 6 decimals

	s.Require().True(s.Run("Setup SPL Token Infrastructure", func() {
		s.Require().True(s.Run("Create Test SPL Token Mint", func() {
			var err error
			tokenMint, err = s.createSPLTokenMint(ctx, SPLTokenDecimals)
			s.Require().NoError(err)
			s.T().Logf("Created test SPL token mint: %s", tokenMint.String())
		}))

		s.Require().True(s.Run("Derive ICS27 Account PDA", func() {
			ics27AccountPDA, _ = solana.GMPAccountPDA(CosmosClientID, cosmosUser.FormattedAddress(), []byte{})
			s.T().Logf("ICS27 Account PDA: %s", ics27AccountPDA.String())
		}))

		s.Require().True(s.Run("Create and Fund Token Account", func() {
			var err error
			sourceTokenAccount, err = s.createTokenAccount(ctx, tokenMint, ics27AccountPDA)
			s.Require().NoError(err)

			err = s.mintTokensTo(ctx, tokenMint, sourceTokenAccount, tokenAmount)
			s.Require().NoError(err)
			s.T().Logf("Created and funded source token account: %s", sourceTokenAccount.String())
		}))
	}))

	var cosmosGMPTxHash []byte
	var recipientWallet *solanago.Wallet
	var destTokenAccount solanago.PublicKey

	s.Require().True(s.Run("Send GMP call from Cosmos with short timeout", func() {
		// Using 35 seconds to allow packet to timeout quickly for test purposes
		timeout := uint64(time.Now().Add(35 * time.Second).Unix())

		// Build SPL transfer instruction
		var err error
		recipientWallet, err = s.SolanaChain.CreateAndFundWallet()
		s.Require().NoError(err)

		destTokenAccount, err = s.createTokenAccount(ctx, tokenMint, recipientWallet.PublicKey())
		s.Require().NoError(err)

		splTransferInstruction := token.NewTransferInstruction(
			tokenAmount,
			sourceTokenAccount,
			destTokenAccount,
			ics27AccountPDA,
			[]solanago.PublicKey{},
		).Build()

		instructionData, err := splTransferInstruction.Data()
		s.Require().NoError(err)

		// Create SolanaInstruction protobuf
		solanaInstruction := &solanatypes.SolanaInstruction{
			ProgramId: token.ProgramID.Bytes(),
			Data:      instructionData,
			Accounts: []*solanatypes.SolanaAccountMeta{
				{Pubkey: sourceTokenAccount.Bytes(), IsSigner: false, IsWritable: true},
				{Pubkey: destTokenAccount.Bytes(), IsSigner: false, IsWritable: true},
				{Pubkey: ics27AccountPDA.Bytes(), IsSigner: true, IsWritable: false},
			},
		}

		payload, err := proto.Marshal(solanaInstruction)
		s.Require().NoError(err)

		// Send GMP call with short timeout
		resp, err := s.BroadcastMessages(ctx, simd, cosmosUser, 2_000_000, &gmptypes.MsgSendCall{
			SourceClient:     CosmosClientID,
			Sender:           cosmosUser.FormattedAddress(),
			Receiver:         token.ProgramID.String(),
			Salt:             []byte{},
			Payload:          payload,
			TimeoutTimestamp: timeout,
			Memo:             "timeout test from Cosmos",
			Encoding:         testvalues.Ics27ProtobufEncoding,
		})
		s.Require().NoError(err)

		cosmosGMPTxHashBytes, err := hex.DecodeString(resp.TxHash)
		s.Require().NoError(err)
		cosmosGMPTxHash = cosmosGMPTxHashBytes

		s.T().Logf("Send call transaction (will timeout): %s", resp.TxHash)
	}))

	// Retrieve the recv relay txs before timeout - we'll try to use them after timeout
	var recvRelayTxs *relayertypes.RelayByTxResponse
	s.Require().True(s.Run("Retrieve recv relay txs before timeout", func() {
		s.Require().True(s.Run("Update Tendermint client on Solana", func() {
			updateResp, err := s.RelayerClient.UpdateClient(context.Background(), &relayertypes.UpdateClientRequest{
				SrcChain:    simd.Config().ChainID,
				DstChain:    testvalues.SolanaChainID,
				DstClientId: SolanaClientID,
			})
			s.Require().NoError(err, "Relayer Update Client failed")
			s.Require().NotEmpty(updateResp.Tx, "Relayer Update client should return transaction")

			s.SolanaChain.SubmitChunkedUpdateClient(ctx, s.T(), s.Require(), updateResp, s.SolanaUser)
		}))

		resp, err := s.RelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
			SrcChain:    simd.Config().ChainID,
			DstChain:    testvalues.SolanaChainID,
			SourceTxIds: [][]byte{cosmosGMPTxHash},
			SrcClientId: CosmosClientID,
			DstClientId: SolanaClientID,
		})
		s.Require().NoError(err)
		recvRelayTxs = resp
		s.T().Log("Retrieved recv relay transactions before timeout")
	}))

	// Sleep for 40 seconds to let the packet timeout (timeout is set to 35 seconds)
	s.T().Log("Sleeping 40 seconds to let packet timeout...")
	time.Sleep(40 * time.Second)

	s.Require().True(s.Run("Relay timeout back to Cosmos", func() {
		s.Require().True(s.Run("Relay timeout transaction", func() {
			resp, err := s.RelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
				SrcChain:     testvalues.SolanaChainID,
				DstChain:     simd.Config().ChainID,
				TimeoutTxIds: [][]byte{cosmosGMPTxHash},
				SrcClientId:  SolanaClientID,
				DstClientId:  CosmosClientID,
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Tx, "Relay should return transaction")

			txResp, err := s.BroadcastSdkTxBody(ctx, simd, s.CosmosUsers[0], 500_000, resp.Tx)
			s.Require().NoError(err)
			s.T().Logf("Timeout transaction broadcasted: %s", txResp.TxHash)

			s.T().Log("Timeout successfully processed on Cosmos")
		}))

		s.Require().True(s.Run("Verify timeout effects", func() {
			s.Require().True(s.Run("Verify packet commitment deleted on Cosmos", func() {
				// First GMP packet from Cosmos should have sequence 1
				_, err := e2esuite.GRPCQuery[channeltypesv2.QueryPacketCommitmentResponse](ctx, simd, &channeltypesv2.QueryPacketCommitmentRequest{
					ClientId: CosmosClientID,
					Sequence: 1,
				})
				s.Require().ErrorContains(err, "packet commitment hash not found")
				s.T().Log("Packet commitment successfully deleted from Cosmos")
			}))

			s.Require().True(s.Run("Verify SPL token balances unchanged", func() {
				// Source account should still have all tokens (transfer never executed)
				sourceBalance, err := s.getTokenBalance(ctx, sourceTokenAccount)
				s.Require().NoError(err)
				s.Require().Equal(tokenAmount, sourceBalance, "Source token account should retain all tokens after timeout")
				s.T().Logf("Source token account balance: %d (unchanged)", sourceBalance)

				// Destination account should have 0 tokens (never received)
				destBalance, err := s.getTokenBalance(ctx, destTokenAccount)
				s.Require().NoError(err)
				s.Require().Equal(uint64(0), destBalance, "Destination token account should have 0 tokens after timeout")
				s.T().Logf("Destination token account balance: %d (no transfer occurred)", destBalance)
			}))

			s.Require().True(s.Run("Verify recvPacket fails on Solana after timeout", func() {
				_, err := s.SolanaChain.SubmitChunkedRelayPackets(ctx, s.T(), recvRelayTxs, s.SolanaUser)
				s.Require().Error(err)
			}))
		}))
	}))
}

// Test_GMPFailedExecutionFromCosmos verifies that CPI errors cause immediate transaction failure
//
// Test Scenario:
// 1. Cosmos chain sends a GMP packet to Solana requesting an SPL token transfer
// 2. The ICS27 account is funded with insufficient tokens (5 tokens)
// 3. The GMP payload requests a transfer of 10 tokens (more than available)
// 4. When the ICS27 GMP program attempts the transfer via CPI, it fails
// 5. The entire transaction aborts - no error acknowledgment is sent back to Cosmos
//
// Solana Architectural Constraint:
// Unlike IBC/EVM where execution errors can be caught and returned as error acknowledgments,
// Solana CPIs (Cross-Program Invocations) fail atomically. When invoke() or invoke_signed()
// fails, the entire transaction aborts immediately - by design to maintain atomicity.
//
// Technical Details:
// CPI errors cannot be handled in Solana programs - when invoke() or invoke_signed()
// fails, the entire transaction aborts immediately. This is by design to maintain
// transaction atomicity.
//
// Runtime Implementation:
// The error propagation happens at the VM/runtime level. When a child program returns
// an error, it propagates immediately via the ? operator in cpi_common():
// https://github.com/anza-xyz/agave/blob/6ba8c59466d18ef480680732c89fa076b15843f5/program-runtime/src/cpi.rs#L843
//
// Error propagation flow in process_instruction():
// https://github.com/anza-xyz/agave/blob/6ba8c59466d18ef480680732c89fa076b15843f5/program-runtime/src/invoke_context.rs#L488-L498
//
// Unit Test Proof:
// There's a test that proves CPI errors cause transaction abort even when the Result
// is ignored.
//
// Test setup (expects transaction to fail with Custom(42)):
// https://github.com/anza-xyz/agave/blob/6ba8c59466d18ef480680732c89fa076b15843f5/programs/sbf/tests/programs.rs#L1043-L1049
//
// Parent program IGNORES the invoke() result with "let _ = invoke(...)":
// https://github.com/anza-xyz/agave/blob/6ba8c59466d18ef480680732c89fa076b15843f5/programs/sbf/rust/invoke/src/lib.rs#L604
//
// Child program returns error Custom(42):
// https://github.com/anza-xyz/agave/blob/6ba8c59466d18ef480680732c89fa076b15843f5/programs/sbf/rust/invoked/src/lib.rs#L119
//
// The test confirms that even though the parent ignores the Result, the transaction
// aborts with the child's error. The parent program never gets to execute any code
// after the failed invoke() call - the abort happens at the runtime/VM level.
//
// This is fundamentally different from EVM's try/catch mechanism or Cosmos SDK's error returns.
func (s *IbcEurekaSolanaTestSuite) Test_GMPFailedExecutionFromCosmos() {
	ctx := context.Background()

	s.UseMockWasmClient = true
	s.SetupGMP = true

	s.SetupSuite(ctx)

	simd := s.CosmosChains[0]
	cosmosUser := s.CosmosUsers[0]

	// Setup SPL token infrastructure with insufficient balance
	var tokenMint solanago.PublicKey
	var ics27AccountPDA solanago.PublicKey
	var sourceTokenAccount solanago.PublicKey
	var destTokenAccount solanago.PublicKey
	var recipientWallet *solanago.Wallet

	const (
		insufficientAmount = uint64(5_000_000)  // 5 tokens with 6 decimals
		excessiveAmount    = uint64(10_000_000) // 10 tokens - MORE than available!
	)

	s.Require().True(s.Run("Setup SPL Token Infrastructure", func() {
		s.Require().True(s.Run("Create Test SPL Token Mint", func() {
			var err error
			tokenMint, err = s.createSPLTokenMint(ctx, SPLTokenDecimals)
			s.Require().NoError(err)
			s.T().Logf("Created test SPL token mint: %s (decimals: %d)", tokenMint.String(), SPLTokenDecimals)
		}))

		s.Require().True(s.Run("Derive ICS27 Account PDA", func() {
			ics27AccountPDA, _ = solana.GMPAccountPDA(CosmosClientID, cosmosUser.FormattedAddress(), []byte{})
			s.T().Logf("ICS27 Account PDA for Cosmos user: %s", ics27AccountPDA.String())
		}))

		s.Require().True(s.Run("Create Token Accounts", func() {
			var err error

			// Create source token account (owned by ICS27 PDA)
			sourceTokenAccount, err = s.createTokenAccount(ctx, tokenMint, ics27AccountPDA)
			s.Require().NoError(err)
			s.T().Logf("Created source token account (owned by ICS27 PDA): %s", sourceTokenAccount.String())

			// Create recipient wallet and destination token account
			recipientWallet, err = s.SolanaChain.CreateAndFundWallet()
			s.Require().NoError(err)

			destTokenAccount, err = s.createTokenAccount(ctx, tokenMint, recipientWallet.PublicKey())
			s.Require().NoError(err)
			s.T().Logf("Created destination token account (owned by recipient): %s", destTokenAccount.String())
		}))

		s.Require().True(s.Run("Mint Insufficient Tokens to ICS27 PDA", func() {
			// CRITICAL: Mint ONLY 5 tokens (we'll try to transfer 10 later)
			err := s.mintTokensTo(ctx, tokenMint, sourceTokenAccount, insufficientAmount)
			s.Require().NoError(err)

			balance, err := s.getTokenBalance(ctx, sourceTokenAccount)
			s.Require().NoError(err)
			s.Require().Equal(insufficientAmount, balance)
			s.T().Logf("Minted %d tokens to ICS27 PDA (intentionally insufficient)", insufficientAmount)
		}))
	}))

	// Record initial state
	var initialSourceBalance, initialDestBalance uint64
	var initialNonce uint64

	s.Require().True(s.Run("Record Initial State", func() {
		var err error

		initialSourceBalance, err = s.getTokenBalance(ctx, sourceTokenAccount)
		s.Require().NoError(err)
		s.Require().Equal(insufficientAmount, initialSourceBalance)

		initialDestBalance, err = s.getTokenBalance(ctx, destTokenAccount)
		s.Require().NoError(err)
		s.Require().Equal(uint64(0), initialDestBalance)

		// Try to get initial nonce (will be 0 if account doesn't exist yet)
		initialNonce = s.getICS27AccountNonce(ctx, ics27AccountPDA)

		s.T().Logf("Initial state - Source: %d tokens, Dest: %d tokens, Nonce: %d",
			initialSourceBalance, initialDestBalance, initialNonce)
	}))

	// Send GMP call that will fail (requesting more tokens than available)
	var cosmosGMPTxHash []byte

	s.Require().True(s.Run("Send GMP SPL Transfer (Will Fail)", func() {
		timeout := uint64(time.Now().Add(30 * time.Minute).Unix())

		// Build SPL transfer instruction requesting 10 tokens (but only 5 available!)
		splTransferInstruction := token.NewTransferInstruction(
			excessiveAmount, // Request 10 tokens
			sourceTokenAccount,
			destTokenAccount,
			ics27AccountPDA, // Authority (will be signed by GMP via invoke_signed)
			[]solanago.PublicKey{},
		).Build()

		instructionData, err := splTransferInstruction.Data()
		s.Require().NoError(err)

		// Create SolanaInstruction protobuf
		solanaInstruction := &solanatypes.SolanaInstruction{
			ProgramId: token.ProgramID.Bytes(),
			Data:      instructionData,
			Accounts: []*solanatypes.SolanaAccountMeta{
				{Pubkey: sourceTokenAccount.Bytes(), IsSigner: false, IsWritable: true},
				{Pubkey: destTokenAccount.Bytes(), IsSigner: false, IsWritable: true},
				{Pubkey: ics27AccountPDA.Bytes(), IsSigner: true, IsWritable: false},
			},
			// PayerPosition is nil - no payer injection needed for SPL transfer
		}

		payload, err := proto.Marshal(solanaInstruction)
		s.Require().NoError(err)

		// Send GMP call
		resp, err := s.BroadcastMessages(ctx, simd, cosmosUser, 2_000_000, &gmptypes.MsgSendCall{
			SourceClient:     CosmosClientID,
			Sender:           cosmosUser.FormattedAddress(),
			Receiver:         token.ProgramID.String(),
			Salt:             []byte{},
			Payload:          payload,
			TimeoutTimestamp: timeout,
			Memo:             "SPL transfer that will fail (insufficient balance)",
			Encoding:         testvalues.Ics27ProtobufEncoding,
		})
		s.Require().NoError(err)

		cosmosGMPTxHashBytes, err := hex.DecodeString(resp.TxHash)
		s.Require().NoError(err)
		cosmosGMPTxHash = cosmosGMPTxHashBytes

		s.T().Logf("GMP packet sent (will fail on execution): %s", resp.TxHash)
		s.T().Logf("Requested transfer: %d tokens (available: %d tokens)", excessiveAmount, insufficientAmount)
	}))

	// Relay packet to Solana and execute (will fail due to CPI error)
	s.Require().True(s.Run("Relay and Execute on Solana", func() {
		s.Require().True(s.Run("Update Tendermint client on Solana", func() {
			updateResp, err := s.RelayerClient.UpdateClient(context.Background(), &relayertypes.UpdateClientRequest{
				SrcChain:    simd.Config().ChainID,
				DstChain:    testvalues.SolanaChainID,
				DstClientId: SolanaClientID,
			})
			s.Require().NoError(err, "Relayer Update Client failed")
			s.Require().NotEmpty(updateResp.Tx, "Relayer Update client should return transaction")

			s.SolanaChain.SubmitChunkedUpdateClient(ctx, s.T(), s.Require(), updateResp, s.SolanaUser)
		}))

		s.Require().True(s.Run("Relay packet to Solana (will fail)", func() {
			resp, err := s.RelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
				SrcChain:    simd.Config().ChainID,
				DstChain:    testvalues.SolanaChainID,
				SourceTxIds: [][]byte{cosmosGMPTxHash},
				SrcClientId: CosmosClientID,
				DstClientId: SolanaClientID,
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Tx, "Relay should return transaction")

			// Transaction will fail due to CPI error (insufficient balance for SPL token transfer)
			// Expected error: SPL Token program error indicating insufficient funds
			_, err = s.SolanaChain.SubmitChunkedRelayPackets(ctx, s.T(), resp, s.SolanaUser)
			s.Require().Error(err)
			s.Require().Contains(err.Error(), "insufficient")
		}))
	}))
}

// Test_GMPFailedExecutionFromSolana verifies that Cosmos can handle execution errors gracefully
//
// Test Scenario:
// 1. Solana sends a GMP packet to Cosmos requesting a bank transfer
// 2. The ICS27 account on Cosmos has insufficient balance (0 tokens)
// 3. The GMP payload requests a transfer of tokens (but none available)
// 4. Cosmos receives the packet and attempts execution
// 5. Cosmos catches the execution error and returns an error acknowledgment
// 6. The error ack is relayed back to Solana successfully
//
// Cosmos Architectural Behavior:
// Unlike Solana where CPI errors cause immediate transaction abort, Cosmos SDK applications
// can catch execution errors and return them as error acknowledgments. This allows the IBC
// protocol to complete successfully even when the application-level execution fails.
//
// When a Cosmos SDK message fails (e.g., insufficient balance, invalid recipient), the error
// is caught by the GMP keeper and encoded into an error acknowledgment that gets returned
// to the sending chain. The IBC packet lifecycle completes normally - the packet is received,
// an ack is written, and the sending chain can process the error ack.
//
// This is fundamentally different from Solana's atomic CPI behavior where errors propagate
// through the runtime and abort the entire transaction before any acknowledgment can be written.
func (s *IbcEurekaSolanaTestSuite) Test_GMPFailedExecutionFromSolana() {
	ctx := context.Background()

	s.UseMockWasmClient = true
	s.SetupGMP = true

	s.SetupSuite(ctx)

	simd := s.CosmosChains[0]

	// Create a test Cosmos user to receive the transfer (if it succeeds)
	testAmount := sdk.NewCoins(sdk.NewCoin(simd.Config().Denom, sdkmath.NewInt(CosmosTestAmount)))
	testCosmosUser := s.CreateAndFundCosmosUserWithBalance(ctx, simd, testAmount[0].Amount.Int64())

	var computedAddress sdk.AccAddress
	s.Require().True(s.Run("Compute ICS27 address on Cosmos (will have zero balance)", func() {
		solanaUserAddress := s.SolanaUser.PublicKey().String()

		res, err := e2esuite.GRPCQuery[gmptypes.QueryAccountAddressResponse](ctx, simd, &gmptypes.QueryAccountAddressRequest{
			ClientId: CosmosClientID,
			Sender:   solanaUserAddress,
			Salt:     "",
		})
		s.Require().NoError(err)
		s.Require().NotEmpty(res.AccountAddress)

		computedAddress, err = sdk.AccAddressFromBech32(res.AccountAddress)
		s.Require().NoError(err)

		s.T().Logf("ICS27 account on Cosmos: %s (will have zero balance - execution will fail)", computedAddress.String())
	}))

	// Verify the ICS27 account has zero balance (or doesn't exist yet)
	s.Require().True(s.Run("Verify ICS27 account has insufficient balance", func() {
		resp, err := e2esuite.GRPCQuery[banktypes.QueryBalanceResponse](ctx, simd, &banktypes.QueryBalanceRequest{
			Address: computedAddress.String(),
			Denom:   simd.Config().Denom,
		})
		s.Require().NoError(err)

		balance := int64(0)
		if resp.Balance != nil {
			balance = resp.Balance.Amount.Int64()
		}
		s.T().Logf("ICS27 account balance: %d (insufficient for transfer)", balance)
	}))

	var solanaPacketTxHash string
	s.Require().True(s.Run("Send call from Solana (will fail on Cosmos execution)", func() {
		timeout := uint64(time.Now().Add(30 * time.Minute).Unix())

		var payload []byte
		s.Require().True(s.Run("Prepare GMP payload (bank transfer that will fail)", func() {
			// Try to send testAmount from ICS27 account (which has zero balance)
			msgSend := &banktypes.MsgSend{
				FromAddress: computedAddress.String(),
				ToAddress:   testCosmosUser.FormattedAddress(),
				Amount:      testAmount,
			}

			var err error
			payload, err = gmphelpers.NewPayload_FromProto([]proto.Message{msgSend})
			s.Require().NoError(err)
			s.T().Logf("Encoded GMP payload (%d bytes) - will fail due to insufficient balance", len(payload))
		}))

		var gmpAppStatePDA, routerStatePDA, routerCallerPDA, clientPDA, ibcAppPDA, clientSequencePDA solanago.PublicKey
		s.Require().True(s.Run("Derive required PDAs", func() {
			gmpAppStatePDA, _ = solana.GMPAppStatePDA(GMPPortID)
			routerStatePDA, _ = solana.RouterStatePDA()
			routerCallerPDA, _ = solana.GMPRouterCallerPDA()
			clientPDA, _ = solana.RouterClientPDA(SolanaClientID)
			ibcAppPDA, _ = solana.RouterIBCAppPDA(GMPPortID)
			clientSequencePDA, _ = solana.RouterClientSequencePDA(SolanaClientID)
		}))

		var packetCommitmentPDA solanago.PublicKey
		var nextSequence uint64
		s.Require().True(s.Run("Get next sequence number and packet commitment PDA", func() {
			var err error
			nextSequence, err = s.SolanaChain.GetNextSequenceNumber(ctx, clientSequencePDA)
			s.Require().NoError(err)

			packetCommitmentPDA, _ = solana.RouterPacketCommitmentPDA(SolanaClientID, nextSequence)
			s.T().Logf("Using sequence number: %d", nextSequence)
		}))

		var sendCallInstruction solanago.Instruction
		s.Require().True(s.Run("Build send_call instruction", func() {
			var err error
			sendCallInstruction, err = ics27_gmp.NewSendCallInstruction(
				ics27_gmp.SendCallMsg{
					SourceClient:     SolanaClientID,
					TimeoutTimestamp: int64(timeout),
					Receiver:         solanago.PublicKey{},
					Salt:             []byte{},
					Payload:          payload,
					Memo:             "send from Solana to Cosmos (will fail on execution)",
				},
				gmpAppStatePDA,
				s.SolanaUser.PublicKey(),
				s.SolanaUser.PublicKey(),
				ics26_router.ProgramID,
				routerStatePDA,
				clientSequencePDA,
				packetCommitmentPDA,
				routerCallerPDA,
				ibcAppPDA,
				clientPDA,
				solanago.SystemProgramID,
			)
			s.Require().NoError(err)
		}))

		s.Require().True(s.Run("Broadcast transaction", func() {
			tx, err := s.SolanaChain.NewTransactionFromInstructions(
				s.SolanaUser.PublicKey(),
				sendCallInstruction,
			)
			s.Require().NoError(err)

			sig, err := s.SolanaChain.SignAndBroadcastTxWithRetry(ctx, tx, s.SolanaUser)
			s.Require().NoError(err)
			s.Require().NotEmpty(sig)

			solanaPacketTxHash = sig.String()
			s.T().Logf("Send call transaction: %s", solanaPacketTxHash)
		}))
	}))

	// Relay packet to Cosmos and execute (will return error ack)
	var cosmosRecvTxHash string
	s.Require().True(s.Run("Receive packet in Cosmos (execution will fail gracefully)", func() {
		var recvRelayTx []byte
		s.Require().True(s.Run("Retrieve relay tx", func() {
			txHashBytes := []byte(solanaPacketTxHash)

			resp, err := s.RelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
				SrcChain:    testvalues.SolanaChainID,
				DstChain:    simd.Config().ChainID,
				SourceTxIds: [][]byte{txHashBytes},
				SrcClientId: SolanaClientID,
				DstClientId: CosmosClientID,
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Tx)

			recvRelayTx = resp.Tx
		}))

		s.Require().True(s.Run("Submit relay tx to Cosmos", func() {
			receipt := s.MustBroadcastSdkTxBody(ctx, simd, s.CosmosUsers[0], 2_000_000, recvRelayTx)
			s.T().Logf("Recv packet tx result: code=%d, log=%s, gas=%d", receipt.Code, receipt.RawLog, receipt.GasUsed)

			// The IBC packet should be received successfully (code=0)
			// even though the application-level execution failed
			s.Require().Equal(uint32(0), receipt.Code, "Recv packet should succeed (IBC layer)")
			s.Require().NotEmpty(receipt.TxHash)

			cosmosRecvTxHash = receipt.TxHash
			s.T().Logf("Packet received on Cosmos, execution failed, error ack written: %s", cosmosRecvTxHash)
		}))
	}))

	// Relay error acknowledgment back to Solana
	s.Require().True(s.Run("Relay error acknowledgment to Solana", func() {
		s.Require().True(s.Run("Update Tendermint client on Solana", func() {
			updateResp, err := s.RelayerClient.UpdateClient(context.Background(), &relayertypes.UpdateClientRequest{
				SrcChain:    simd.Config().ChainID,
				DstChain:    testvalues.SolanaChainID,
				DstClientId: SolanaClientID,
			})
			s.Require().NoError(err, "Relayer Update Client failed")
			s.Require().NotEmpty(updateResp.Tx, "Relayer Update client should return transaction")

			s.SolanaChain.SubmitChunkedUpdateClient(ctx, s.T(), s.Require(), updateResp, s.SolanaUser)
		}))

		s.Require().True(s.Run("Get acknowledgment relay transactions", func() {
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

			sig, err := s.SolanaChain.SubmitChunkedRelayPackets(ctx, s.T(), resp, s.SolanaUser)
			s.Require().NoError(err)
			s.T().Logf("Error acknowledgment successfully relayed to Solana: %s", sig)
		}))
	}))

	// Verify the packet commitment was deleted (ack processed)
	s.Require().True(s.Run("Verify packet commitment deleted on Solana", func() {
		// Derive packet commitment PDA for the sequence we used
		clientSequencePDA, _ := solana.RouterClientSequencePDA(SolanaClientID)

		// Get the sequence we used (it was incremented after send)
		sequence, err := s.SolanaChain.GetNextSequenceNumber(ctx, clientSequencePDA)
		s.Require().NoError(err)

		// The sequence we used was (current - 1)
		usedSequence := sequence - 1

		s.SolanaChain.VerifyPacketCommitmentDeleted(ctx, s.T(), s.Require(), SolanaClientID, usedSequence)
		s.T().Logf("Verified packet commitment deleted for sequence %d", usedSequence)
	}))
}
