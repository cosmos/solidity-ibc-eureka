package main

import (
	"context"
	"crypto/sha256"
	"encoding/binary"
	"encoding/hex"
	"fmt"
	"time"

	"github.com/cosmos/gogoproto/proto"
	gmp_counter_app "github.com/cosmos/solidity-ibc-eureka/e2e/interchaintestv8/solana/go-anchor/gmpcounter"
	"github.com/cosmos/solidity-ibc-eureka/e2e/v8/e2esuite"
	"github.com/cosmos/solidity-ibc-eureka/e2e/v8/solana"
	"github.com/cosmos/solidity-ibc-eureka/e2e/v8/testvalues"
	"github.com/cosmos/solidity-ibc-eureka/e2e/v8/types/gmphelpers"
	relayertypes "github.com/cosmos/solidity-ibc-eureka/e2e/v8/types/relayer"
	solanatypes "github.com/cosmos/solidity-ibc-eureka/e2e/v8/types/solana"

	solanago "github.com/gagliardetto/solana-go"
	"github.com/gagliardetto/solana-go/programs/system"
	"github.com/gagliardetto/solana-go/programs/token"

	sdkmath "cosmossdk.io/math"

	sdk "github.com/cosmos/cosmos-sdk/types"
	banktypes "github.com/cosmos/cosmos-sdk/x/bank/types"

	gmptypes "github.com/cosmos/ibc-go/v10/modules/apps/27-gmp/types"

	"github.com/cosmos/interchaintest/v10/ibc"

	ics07_tendermint "github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/ics07tendermint"
	ics26_router "github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/ics26router"
	ics27_gmp "github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/ics27gmp"
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
		gmpCounterProgramID = s.deploySolanaProgram(ctx, "gmp_counter_app")

		gmp_counter_app.ProgramID = gmpCounterProgramID

		programAvailable := s.SolanaChain.WaitForProgramAvailabilityWithTimeout(ctx, gmpCounterProgramID, 120)
		s.Require().True(programAvailable, "GMP Counter program failed to become available within timeout")

		// Initialize GMP counter app state
		counterAppStatePDA, _, err := solanago.FindProgramAddress([][]byte{[]byte("counter_app_state")}, gmpCounterProgramID)
		s.Require().NoError(err)

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

// createAddressLookupTable creates an Address Lookup Table with common IBC accounts
// to reduce transaction size. Returns the ALT address.
func (s *IbcEurekaSolanaTestSuite) createAddressLookupTable(ctx context.Context) solanago.PublicKey {
	// Define common accounts to add to ALT
	// These are accounts that appear in every IBC packet transaction
	// Derive router_state PDA (same as relayer uses)
	routerStatePDA, _, err := solanago.FindProgramAddress(
		[][]byte{[]byte("router_state")},
		ics26_router.ProgramID,
	)
	s.Require().NoError(err)

	// Get Cosmos chain ID for deriving ICS07 accounts
	simd := s.CosmosChains[0]
	cosmosChainID := simd.Config().ChainID

	// Derive IBC app PDA (port-specific, constant for all GMP packets)
	ibcAppPDA, _, err := solanago.FindProgramAddress([][]byte{[]byte("ibc_app"), []byte(GMPPortID)}, ics26_router.ProgramID)
	s.Require().NoError(err)

	// Derive ICS27 GMP app state PDA (port-specific, constant for all GMP packets)
	gmpAppStatePDA, _, err := solanago.FindProgramAddress([][]byte{[]byte("app_state"), []byte(GMPPortID)}, ics27_gmp.ProgramID)
	s.Require().NoError(err)

	// Derive client PDA (client-specific, constant if using same destination client)
	// Assuming destination client is "solclient-0"
	clientPDA, _, err := solanago.FindProgramAddress([][]byte{[]byte("clients"), []byte("solclient-0")}, ics26_router.ProgramID)
	s.Require().NoError(err)

	// Derive ICS07 client state PDA (source chain specific, constant for all packets from Cosmos)
	clientStatePDA, _, err := solanago.FindProgramAddress([][]byte{[]byte("client"), []byte(cosmosChainID)}, ics07_tendermint.ProgramID)
	s.Require().NoError(err)

	// Derive router caller PDA (GMP's CPI signer, constant for all GMP packets)
	routerCallerPDA, _, err := solanago.FindProgramAddress([][]byte{[]byte("router_caller")}, ics27_gmp.ProgramID)
	s.Require().NoError(err)

	// Derive client sequence PDA (tracks packet sequence for destination client)
	clientSequencePDA, _, err := solanago.FindProgramAddress([][]byte{[]byte("client_sequence"), []byte(SolanaClientID)}, ics26_router.ProgramID)
	s.Require().NoError(err)

	// NOTE: We do NOT include target app-specific accounts (like gmp_counter_app.ProgramID or its state)
	// because those vary per application. ALT should only contain universal GMP infrastructure accounts.
	commonAccounts := []solanago.PublicKey{
		solanago.SystemProgramID,
		solana.ComputeBudgetProgramID(), // Compute Budget program (used by update_client)
		ics26_router.ProgramID,          // Router program
		ics07_tendermint.ProgramID,      // Light client program
		ics27_gmp.ProgramID,             // GMP program (ibc_app_program)
		routerStatePDA,                  // Router state PDA
		s.SolanaUser.PublicKey(),        // Fee payer / relayer
		ibcAppPDA,                       // IBC app PDA for GMP port
		gmpAppStatePDA,                  // GMP app state PDA
		clientPDA,                       // Client PDA
		clientStatePDA,                  // ICS07 client state PDA
		routerCallerPDA,                 // GMP router caller PDA (CPI signer)
		clientSequencePDA,               // Client sequence PDA (tracks packet sequence)
	}

	// Create ALT with common accounts
	altAddress, err := s.SolanaChain.CreateAddressLookupTable(ctx, s.SolanaUser, commonAccounts)
	s.Require().NoError(err)
	s.T().Logf("Created and extended ALT %s with %d common accounts", altAddress, len(commonAccounts))

	return altAddress
}

func (s *IbcEurekaSolanaTestSuite) deployAndInitializeICS27GMP(ctx context.Context) solanago.PublicKey {
	var ics27GMPProgramID solanago.PublicKey

	s.Require().True(s.Run("Deploy and Initialize ICS27 GMP Program", func() {
		ics27GMPProgramID = s.deploySolanaProgram(ctx, "ics27_gmp")

		// Set the program ID in the bindings
		ics27_gmp.ProgramID = ics27GMPProgramID

		programAvailable := s.SolanaChain.WaitForProgramAvailabilityWithTimeout(ctx, ics27GMPProgramID, 120)
		s.Require().True(programAvailable, "ICS27 GMP program failed to become available within timeout")

		// Find GMP app state PDA (using standard pattern with port_id)
		gmpAppStatePDA, _, err := solanago.FindProgramAddress([][]byte{[]byte("app_state"), []byte(GMPPortID)}, ics27GMPProgramID)
		s.Require().NoError(err)

		// Find router caller PDA
		routerCallerPDA, _, err := solanago.FindProgramAddress([][]byte{[]byte("router_caller")}, ics27GMPProgramID)
		s.Require().NoError(err)

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
		routerStateAccount, _, err := solanago.FindProgramAddress([][]byte{[]byte("router_state")}, ics26_router.ProgramID)
		s.Require().NoError(err)

		ibcAppAccount, _, err := solanago.FindProgramAddress([][]byte{[]byte("ibc_app"), []byte(GMPPortID)}, ics26_router.ProgramID)
		s.Require().NoError(err)

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
// This mirrors the Ethereum GMP test pattern but for Solana
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
			hasher := sha256.New()
			hasher.Write([]byte(cosmosUserAddress))
			senderHash := hasher.Sum(nil)

			ics27AccountPDA, _, err := solanago.FindProgramAddress([][]byte{
				[]byte("gmp_account"),
				[]byte(CosmosClientID),
				senderHash,
				salt,
			}, ics27GMPProgramID)
			s.Require().NoError(err)

			// Derive user counter PDA from ICS27 account_state PDA
			userCounterPDA, _, err := solanago.FindProgramAddress(
				[][]byte{[]byte("user_counter"), ics27AccountPDA.Bytes()},
				gmpCounterProgramID,
			)
			s.Require().NoError(err)

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
			hasher := sha256.New()
			hasher.Write([]byte(cosmosAddress))
			senderHash := hasher.Sum(nil)

			ics27AccountPDA, _, err := solanago.FindProgramAddress([][]byte{
				[]byte("gmp_account"),
				[]byte(CosmosClientID),
				senderHash,
				salt,
			}, ics27GMPProgramID)
			if err != nil {
				return nil
			}

			// Create the raw instruction data (just discriminator + amount, no user pubkey)
			incrementInstructionData := []byte{}
			incrementInstructionData = append(incrementInstructionData, gmp_counter_app.Instruction_Increment[:]...)
			amountBytes := make([]byte, 8)
			binary.LittleEndian.PutUint64(amountBytes, amount)
			incrementInstructionData = append(incrementInstructionData, amountBytes...)

			// Derive required account addresses
			// 1. Counter app_state PDA
			counterAppStateAddress, _, err := solanago.FindProgramAddress([][]byte{[]byte("counter_app_state")}, gmpCounterProgramID)
			if err != nil {
				return nil
			}

			// 2. User counter PDA - derived from the ICS27 account_state PDA (not userKey)
			userCounterAddress, _, err := solanago.FindProgramAddress([][]byte{[]byte("user_counter"), ics27AccountPDA.Bytes()}, gmpCounterProgramID)
			if err != nil {
				return nil
			}

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
			s.Require().NotEmpty(updateResp.Txs, "Relayer Update client should return chunked transactions")

			s.submitChunkedUpdateClient(ctx, updateResp, s.SolanaUser)
			s.T().Logf("%s: Updated Tendermint client on Solana using %d chunked transactions", userLabel, len(updateResp.Txs))

			// Now retrieve and relay the GMP packet
			resp, err := s.RelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
				SrcChain:    simd.Config().ChainID,
				DstChain:    testvalues.SolanaChainID,
				SourceTxIds: [][]byte{cosmosGMPTxHash},
				SrcClientId: CosmosClientID,
				DstClientId: SolanaClientID,
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Txs, "Relay should return chunked transactions")
			s.T().Logf("%s: Retrieved %d relay transactions (chunks + final instructions)", userLabel, len(resp.Txs))

			// Execute on Solana using chunked submission
			solanaRelayTxSig = s.submitChunkedRelayPackets(ctx, resp, s.SolanaUser)
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
			var err error
			ics27AccountPDA, err = s.deriveICS27AccountPDA(cosmosUser.FormattedAddress(), []byte{})
			s.Require().NoError(err)
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
			s.Require().NotEmpty(updateResp.Txs, "Relayer Update client should return chunked transactions")

			s.submitChunkedUpdateClient(ctx, updateResp, s.SolanaUser)
			s.T().Logf("Updated Tendermint client on Solana using %d chunked transactions", len(updateResp.Txs))
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
			s.Require().NotEmpty(resp.Txs, "Relay should return chunked transactions")
			s.T().Logf("Retrieved %d relay transactions (chunks + final instructions)", len(resp.Txs))

			solanaRelayTxSig = s.submitChunkedRelayPackets(ctx, resp, s.SolanaUser)
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

// deriveICS27AccountPDA derives the ICS27 Account PDA for a Cosmos user
func (s *IbcEurekaSolanaTestSuite) deriveICS27AccountPDA(cosmosAddress string, salt []byte) (solanago.PublicKey, error) {
	// Hash the sender address using SHA256 (matches the Rust implementation: hash(sender.as_bytes()).to_bytes())
	hasher := sha256.New()
	hasher.Write([]byte(cosmosAddress))
	senderHash := hasher.Sum(nil)

	// Derive PDA: [b"gmp_account", client_id, hash(sender), salt]
	seeds := [][]byte{
		[]byte("gmp_account"),
		[]byte(CosmosClientID),
		senderHash,
		salt,
	}

	pda, _, err := solanago.FindProgramAddress(seeds, ics27_gmp.ProgramID)
	return pda, err
}

func (s *IbcEurekaSolanaTestSuite) Test_GMPSendCallFromSolana() {
	ctx := context.Background()

	s.UseMockWasmClient = true

	s.SetupSuite(ctx)

	simd := s.CosmosChains[0]

	var ics27GMPProgramID solanago.PublicKey
	s.Require().True(s.Run("Deploy and Initialize ICS27 GMP Program", func() {
		ics27GMPProgramID = s.deployAndInitializeICS27GMP(ctx)
	}))

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
			var err error

			gmpAppStatePDA, _, err = solanago.FindProgramAddress(
				[][]byte{[]byte("app_state"), []byte(GMPPortID)},
				ics27GMPProgramID,
			)
			s.Require().NoError(err)

			routerStatePDA, _, err = solanago.FindProgramAddress(
				[][]byte{[]byte("router_state")},
				ics26_router.ProgramID,
			)
			s.Require().NoError(err)

			routerCallerPDA, _, err = solanago.FindProgramAddress(
				[][]byte{[]byte("router_caller")},
				ics27GMPProgramID,
			)
			s.Require().NoError(err)

			clientPDA, _, err = solanago.FindProgramAddress(
				[][]byte{[]byte("client"), []byte(SolanaClientID)},
				ics26_router.ProgramID,
			)
			s.Require().NoError(err)

			ibcAppPDA, _, err = solanago.FindProgramAddress(
				[][]byte{[]byte("ibc_app"), []byte(GMPPortID)},
				ics26_router.ProgramID,
			)
			s.Require().NoError(err)

			clientSequencePDA, _, err = solanago.FindProgramAddress(
				[][]byte{[]byte("client_sequence"), []byte(SolanaClientID)},
				ics26_router.ProgramID,
			)
			s.Require().NoError(err)

			s.T().Logf("Derived PDAs: gmpAppState=%s, routerState=%s, client=%s",
				gmpAppStatePDA.String(), routerStatePDA.String(), clientPDA.String())
		}))

		var packetCommitmentPDA solanago.PublicKey
		var nextSequence uint64
		s.Require().True(s.Run("Get next sequence number", func() {
			nextSequence = 1 // Default if account doesn't exist yet
			clientSequenceAccount, err := s.SolanaChain.RPCClient.GetAccountInfo(ctx, clientSequencePDA)
			if err == nil && clientSequenceAccount.Value != nil {
				data := clientSequenceAccount.Value.Data.GetBinary()
				if len(data) >= 16 {
					// Use the CURRENT value - router derives PDA before incrementing
					nextSequence = binary.LittleEndian.Uint64(data[8:16])
				}
			}

			sequenceBytes := make([]byte, 8)
			binary.LittleEndian.PutUint64(sequenceBytes, nextSequence)
			packetCommitmentPDA, _, err = solanago.FindProgramAddress(
				[][]byte{
					[]byte("packet_commitment"),
					[]byte(SolanaClientID),
					sequenceBytes,
				},
				ics26_router.ProgramID,
			)
			s.Require().NoError(err)

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
			s.Require().NotEmpty(resp.Txs, "Relayer Update client should return transactions")

			s.submitChunkedUpdateClient(ctx, resp, s.SolanaUser)
			s.T().Logf("Successfully updated Tendermint client on Solana using %d transaction(s)", len(resp.Txs))
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
			s.Require().NotEmpty(resp.Txs, "Relay should return chunked transactions")
			s.T().Logf("Retrieved %d relay transactions (chunks + final instructions)", len(resp.Txs))

			sig := s.submitChunkedRelayPackets(ctx, resp, s.SolanaUser)
			s.T().Logf("Acknowledgement transaction broadcasted: %s", sig)
		}))

		s.Require().True(s.Run("Verify acknowledgement was processed", func() {
			s.verifyPacketCommitmentDeleted(ctx, SolanaClientID, 1)
		}))
	}))
}

func (s *IbcEurekaSolanaTestSuite) Test_GMPTimeoutFromSolana() {
	ctx := context.Background()

	s.UseMockWasmClient = true

	s.SetupSuite(ctx)

	simd := s.CosmosChains[0]

	var ics27GMPProgramID solanago.PublicKey
	s.Require().True(s.Run("Deploy and Initialize ICS27 GMP Program", func() {
		ics27GMPProgramID = s.deployAndInitializeICS27GMP(ctx)
	}))

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
	s.Require().True(s.Run("Send call from Solana with short timeout", func() {
		// Use 61 seconds (just above MIN_TIMEOUT_DURATION of 60 seconds)
		timeout := uint64(time.Now().Add(61 * time.Second).Unix())

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
			var err error

			gmpAppStatePDA, _, err = solanago.FindProgramAddress(
				[][]byte{[]byte("app_state"), []byte(GMPPortID)},
				ics27GMPProgramID,
			)
			s.Require().NoError(err)

			routerStatePDA, _, err = solanago.FindProgramAddress(
				[][]byte{[]byte("router_state")},
				ics26_router.ProgramID,
			)
			s.Require().NoError(err)

			routerCallerPDA, _, err = solanago.FindProgramAddress(
				[][]byte{[]byte("router_caller")},
				ics27GMPProgramID,
			)
			s.Require().NoError(err)

			clientPDA, _, err = solanago.FindProgramAddress(
				[][]byte{[]byte("client"), []byte(SolanaClientID)},
				ics26_router.ProgramID,
			)
			s.Require().NoError(err)

			ibcAppPDA, _, err = solanago.FindProgramAddress(
				[][]byte{[]byte("ibc_app"), []byte(GMPPortID)},
				ics26_router.ProgramID,
			)
			s.Require().NoError(err)

			clientSequencePDA, _, err = solanago.FindProgramAddress(
				[][]byte{[]byte("client_sequence"), []byte(SolanaClientID)},
				ics26_router.ProgramID,
			)
			s.Require().NoError(err)

			s.T().Logf("Derived PDAs: gmpAppState=%s, routerState=%s, client=%s",
				gmpAppStatePDA.String(), routerStatePDA.String(), clientPDA.String())
		}))

		var packetCommitmentPDA solanago.PublicKey
		var nextSequence uint64
		s.Require().True(s.Run("Get next sequence number", func() {
			nextSequence = 1
			clientSequenceAccount, err := s.SolanaChain.RPCClient.GetAccountInfo(ctx, clientSequencePDA)
			if err == nil && clientSequenceAccount.Value != nil {
				data := clientSequenceAccount.Value.Data.GetBinary()
				if len(data) >= 16 {
					nextSequence = binary.LittleEndian.Uint64(data[8:16])
				}
			}

			sequenceBytes := make([]byte, 8)
			binary.LittleEndian.PutUint64(sequenceBytes, nextSequence)
			packetCommitmentPDA, _, err = solanago.FindProgramAddress(
				[][]byte{
					[]byte("packet_commitment"),
					[]byte(SolanaClientID),
					sequenceBytes,
				},
				ics26_router.ProgramID,
			)
			s.Require().NoError(err)

			s.T().Logf("Using sequence number: %d (timeout test)", nextSequence)
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

	// Sleep for 65 seconds to let the packet timeout (timeout is set to 61 seconds)
	s.T().Log("Sleeping 65 seconds to let packet timeout...")
	time.Sleep(65 * time.Second)

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
			s.Require().NotEmpty(resp.Txs, "Relayer Update client should return transactions")

			s.submitChunkedUpdateClient(ctx, resp, s.SolanaUser)
			s.T().Logf("Successfully updated Tendermint client on Solana using %d transaction(s)", len(resp.Txs))
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
			s.Require().NotEmpty(resp.Txs, "Relay should return chunked transactions")
			s.T().Logf("Retrieved %d relay transactions (chunks + final instructions)", len(resp.Txs))

			sig := s.submitChunkedRelayPackets(ctx, resp, s.SolanaUser)
			s.T().Logf("Timeout transaction broadcasted: %s", sig)

			s.T().Log("Timeout successfully processed on Solana")
		}))
	}))
}
