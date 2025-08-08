package tendermint_light_client_fixtures

import (
	"context"
	"encoding/hex"
	"encoding/json"
	"os"
	"path/filepath"
	"time"

	"github.com/cosmos/gogoproto/proto"

	"github.com/cosmos/cosmos-sdk/codec/types"
	txtypes "github.com/cosmos/cosmos-sdk/types/tx"

	clienttypes "github.com/cosmos/ibc-go/v10/modules/core/02-client/types"
	ibctmtypes "github.com/cosmos/ibc-go/v10/modules/light-clients/07-tendermint"

	"github.com/strangelove-ventures/interchaintest/v8/chain/cosmos"
)

// UpdateClientFixtureGenerator handles generation of update client test scenarios
type UpdateClientFixtureGenerator struct {
	generator FixtureGeneratorInterface
}

// NewUpdateClientFixtureGenerator creates a new update client fixture generator
func NewUpdateClientFixtureGenerator(generator FixtureGeneratorInterface) *UpdateClientFixtureGenerator {
	return &UpdateClientFixtureGenerator{
		generator: generator,
	}
}

// GenerateMultipleUpdateClientScenarios generates multiple test scenarios
func (g *UpdateClientFixtureGenerator) GenerateMultipleUpdateClientScenarios(ctx context.Context, chainA *cosmos.CosmosChain, updateTxBodyBz []byte) {
	if !g.generator.IsEnabled() {
		return
	}

	g.generator.LogInfo("🔧 Generating multiple update client scenarios")

	// Extract the real update client message from the transaction
	g.generator.LogInfo("🔍 Parsing update client transaction")
	msgUpdateClient := g.extractUpdateClientMessage(updateTxBodyBz)
	g.generator.LogInfof("📊 Found MsgUpdateClient for client: %s", msgUpdateClient.ClientId)

	// Generate the happy path scenario using real transaction data
	g.generateHappyPathScenario(ctx, chainA, msgUpdateClient.ClientMessage)

	// Generate malformed client message scenario based on the real data
	g.generateMalformedClientMessageScenario(ctx, chainA)

	// Generate additional edge case scenarios
	g.generateExpiredHeaderScenario(ctx, chainA)
	g.generateFutureTimestampScenario(ctx, chainA)
	g.generateWrongTrustedHeightScenario(ctx, chainA)
	g.generateInvalidProtobufScenario()

	g.generator.LogInfo("✅ Multiple update client scenarios generated successfully")
}

func (g *UpdateClientFixtureGenerator) extractUpdateClientMessage(txBodyBz []byte) *clienttypes.MsgUpdateClient {
	var txBody txtypes.TxBody
	err := proto.Unmarshal(txBodyBz, &txBody)
	g.generator.RequireNoError(err)
	g.generator.RequireLen(txBody.Messages, 1, "Expected exactly one message in update client tx")

	var msgUpdateClient clienttypes.MsgUpdateClient
	err = proto.Unmarshal(txBody.Messages[0].Value, &msgUpdateClient)
	g.generator.RequireNoError(err)
	g.generator.RequireNotNil(msgUpdateClient.ClientMessage)

	return &msgUpdateClient
}

func (g *UpdateClientFixtureGenerator) generateHappyPathScenario(ctx context.Context, chainA *cosmos.CosmosChain, clientMessage *types.Any) {
	g.generator.LogInfo("🔧 Generating happy path scenario")

	// Get the client state
	tmClientState := g.generator.QueryTendermintClientState(ctx, chainA)
	clientStateMap := g.generator.ConvertClientStateToFixtureFormat(tmClientState, chainA.Config().ChainID)

	// Get the consensus state (this would be the trusted state)
	tmConsensusState := g.generator.QueryTendermintConsensusState(ctx, chainA)
	consensusStateMap := g.generator.ConvertConsensusStateToFixtureFormat(tmConsensusState, chainA.Config().ChainID)

	// Process the real update client message from the transaction
	realUpdateMessage := g.convertUpdateClientMessageToFixtureFormat(clientMessage)

	// Create the unified fixture
	unifiedFixture := map[string]interface{}{
		"scenario":                "happy_path",
		"client_state":            clientStateMap,
		"trusted_consensus_state": consensusStateMap,
		"update_client_message":   realUpdateMessage,
		"metadata":                g.generator.CreateUnifiedMetadata("happy_path", tmClientState.ChainId),
	}

	filename := filepath.Join(g.generator.GetFixtureDir(), "update_client_happy_path.json")
	g.generator.SaveJsonFixture(filename, unifiedFixture)
	g.generator.LogInfof("💾 Happy path scenario fixture saved: %s", filename)
}

func (g *UpdateClientFixtureGenerator) generateMalformedClientMessageScenario(ctx context.Context, chainA *cosmos.CosmosChain) {
	g.generator.LogInfo("🔧 Generating malformed client message scenario")

	// Get valid client state and consensus state (same as happy path)
	tmClientState := g.generator.QueryTendermintClientState(ctx, chainA)
	clientStateMap := g.generator.ConvertClientStateToFixtureFormat(tmClientState, chainA.Config().ChainID)

	tmConsensusState := g.generator.QueryTendermintConsensusState(ctx, chainA)
	consensusStateMap := g.generator.ConvertConsensusStateToFixtureFormat(tmConsensusState, chainA.Config().ChainID)

	// Load the happy path fixture to base the malformed one on
	happyPathFile := filepath.Join(g.generator.GetFixtureDir(), "update_client_happy_path.json")
	g.generator.RequireFileExists(happyPathFile, "Happy path fixture must exist before generating malformed fixture")

	g.generator.LogInfo("📖 Loading happy path fixture to create malformed version")
	validHex := g.extractHexFromHappyPathFixture(happyPathFile)

	malformedHex := g.corruptSignatureInValidHeader(validHex)

	// Create a malformed update message by corrupting signature bytes from a valid message
	malformedUpdateMessage := map[string]interface{}{
		"client_message_hex": malformedHex,
		"type_url":           "/ibc.lightclients.tendermint.v1.Header",
		"trusted_height":     tmClientState.LatestHeight.RevisionHeight,
		"new_height":         tmClientState.LatestHeight.RevisionHeight + 1,
		"metadata":           g.generator.CreateMetadata("Intentionally malformed Tendermint header for unhappy path testing (signature corruption in valid protobuf structure)"),
	}

	// Create the unified fixture
	unifiedFixture := map[string]interface{}{
		"scenario":                "malformed_client_message",
		"client_state":            clientStateMap,
		"trusted_consensus_state": consensusStateMap,
		"update_client_message":   malformedUpdateMessage,
		"metadata":                g.generator.CreateUnifiedMetadata("malformed_client_message", tmClientState.ChainId),
	}

	filename := filepath.Join(g.generator.GetFixtureDir(), "update_client_malformed_client_message.json")
	g.generator.SaveJsonFixture(filename, unifiedFixture)
	g.generator.LogInfof("💾 Malformed client message scenario fixture saved: %s", filename)
}

func (g *UpdateClientFixtureGenerator) convertUpdateClientMessageToFixtureFormat(clientMessage *types.Any) map[string]interface{} {
	headerBytes := clientMessage.Value

	// Parse the header to extract the new height information
	var tmHeader ibctmtypes.Header
	err := proto.Unmarshal(headerBytes, &tmHeader)
	g.generator.RequireNoError(err, "Failed to parse header for height extraction - fixture generation cannot continue")

	// Validate that we have valid height information
	trustedHeight := tmHeader.TrustedHeight.RevisionHeight
	newHeight := tmHeader.Header.Height

	g.generator.RequireGreater(newHeight, int64(0), "New height must be greater than 0")
	g.generator.RequireGreater(trustedHeight, uint64(0), "Trusted height must be greater than 0")
	g.generator.RequireGreater(newHeight, int64(trustedHeight), "New height must be greater than trusted height")

	return map[string]interface{}{
		"client_message_hex": hex.EncodeToString(headerBytes),
		"type_url":           clientMessage.TypeUrl,
		"trusted_height":     trustedHeight,
		"new_height":         newHeight,
		"metadata":           g.generator.CreateMetadata("Protobuf-encoded Tendermint header for update client"),
	}
}

// extractHexFromHappyPathFixture loads the happy path fixture and extracts the client_message_hex
func (g *UpdateClientFixtureGenerator) extractHexFromHappyPathFixture(filePath string) string {
	data, err := os.ReadFile(filePath)
	g.generator.RequireNoError(err, "Failed to read happy path fixture")

	var fixture map[string]interface{}
	err = json.Unmarshal(data, &fixture)
	g.generator.RequireNoError(err, "Failed to parse happy path fixture JSON")

	updateMessage, ok := fixture["update_client_message"].(map[string]interface{})
	g.generator.RequireTrue(ok, "update_client_message not found in happy path fixture")

	hex, ok := updateMessage["client_message_hex"].(string)
	g.generator.RequireTrue(ok, "client_message_hex not found in happy path fixture")

	return hex
}

// corruptSignatureInValidHeader takes a valid header hex and corrupts signature bytes
// This creates a valid protobuf structure that will deserialize correctly but fail cryptographic verification
func (g *UpdateClientFixtureGenerator) corruptSignatureInValidHeader(validHex string) string {
	// Decode the hex string to bytes
	headerBytes, err := hex.DecodeString(validHex)
	if err != nil {
		g.generator.Fatalf("Failed to decode valid header hex: %v", err)
	}

	// Parse the header first to understand its structure
	var tmHeader ibctmtypes.Header
	err = proto.Unmarshal(headerBytes, &tmHeader)
	if err != nil {
		g.generator.Fatalf("Failed to parse header for corruption: %v", err)
	}

	// Make a copy to avoid modifying the original
	corruptedHeader := tmHeader

	// Corrupt signature data in the commit while preserving the protobuf structure
	if corruptedHeader.SignedHeader != nil && corruptedHeader.Commit != nil {
		commit := corruptedHeader.Commit

		// Corrupt block signature if it exists
		if len(commit.Signatures) > 0 {
			// Corrupt the first signature by flipping one byte
			if len(commit.Signatures[0].Signature) > 10 {
				// Flip a byte in the middle of the signature
				sigPos := len(commit.Signatures[0].Signature) / 2
				commit.Signatures[0].Signature[sigPos] ^= 0xFF
				g.generator.LogInfof("🔧 Corrupted signature byte at position %d in first commit signature", sigPos)
			}
		}

		// Also corrupt the block ID hash if present
		if len(commit.BlockID.Hash) > 0 {
			// Flip one byte in the block hash
			hashPos := len(commit.BlockID.Hash) / 2
			commit.BlockID.Hash[hashPos] ^= 0xFF
			g.generator.LogInfof("🔧 Corrupted block hash byte at position %d", hashPos)
		}
	}

	// Re-marshal the corrupted header
	corruptedBytes, err := proto.Marshal(&corruptedHeader)
	if err != nil {
		g.generator.Fatalf("Failed to marshal corrupted header: %v", err)
	}

	// Verify it can still be parsed (should succeed)
	var testHeader ibctmtypes.Header
	err = proto.Unmarshal(corruptedBytes, &testHeader)
	if err != nil {
		g.generator.Fatalf("Corrupted header failed to parse - corruption was too aggressive: %v", err)
	}

	g.generator.LogInfo("🔧 Header corrupted successfully - still deserializable but signatures are invalid")
	return hex.EncodeToString(corruptedBytes)
}

// generateExpiredHeaderScenario creates a fixture with an expired header (beyond trusting period)
func (g *UpdateClientFixtureGenerator) generateExpiredHeaderScenario(ctx context.Context, chainA *cosmos.CosmosChain) {
	g.generator.LogInfo("🔧 Generating expired header scenario")

	// Get valid client state and consensus state
	tmClientState := g.generator.QueryTendermintClientState(ctx, chainA)
	clientStateMap := g.generator.ConvertClientStateToFixtureFormat(tmClientState, chainA.Config().ChainID)

	tmConsensusState := g.generator.QueryTendermintConsensusState(ctx, chainA)
	consensusStateMap := g.generator.ConvertConsensusStateToFixtureFormat(tmConsensusState, chainA.Config().ChainID)

	// Load the happy path fixture to base the expired one on
	happyPathFile := filepath.Join(g.generator.GetFixtureDir(), "update_client_happy_path.json")
	g.generator.RequireFileExists(happyPathFile)

	validHex := g.extractHexFromHappyPathFixture(happyPathFile)

	// Create an expired header by modifying the timestamp
	expiredHex := g.createExpiredHeader(validHex, int64(tmClientState.TrustingPeriod.Seconds()))

	expiredUpdateMessage := map[string]interface{}{
		"client_message_hex": expiredHex,
		"type_url":           "/ibc.lightclients.tendermint.v1.Header",
		"trusted_height":     tmClientState.LatestHeight.RevisionHeight,
		"new_height":         tmClientState.LatestHeight.RevisionHeight + 1,
		"metadata":           g.generator.CreateMetadata("Expired header - timestamp beyond trusting period"),
	}

	unifiedFixture := map[string]interface{}{
		"scenario":                "expired_header",
		"client_state":            clientStateMap,
		"trusted_consensus_state": consensusStateMap,
		"update_client_message":   expiredUpdateMessage,
		"metadata":                g.generator.CreateUnifiedMetadata("expired_header", tmClientState.ChainId),
	}

	filename := filepath.Join(g.generator.GetFixtureDir(), "update_client_expired_header.json")
	g.generator.SaveJsonFixture(filename, unifiedFixture)
	g.generator.LogInfof("💾 Expired header scenario fixture saved: %s", filename)
}

// generateFutureTimestampScenario creates a fixture with a future timestamp
func (g *UpdateClientFixtureGenerator) generateFutureTimestampScenario(ctx context.Context, chainA *cosmos.CosmosChain) {
	g.generator.LogInfo("🔧 Generating future timestamp scenario")

	tmClientState := g.generator.QueryTendermintClientState(ctx, chainA)
	clientStateMap := g.generator.ConvertClientStateToFixtureFormat(tmClientState, chainA.Config().ChainID)

	tmConsensusState := g.generator.QueryTendermintConsensusState(ctx, chainA)
	consensusStateMap := g.generator.ConvertConsensusStateToFixtureFormat(tmConsensusState, chainA.Config().ChainID)

	happyPathFile := filepath.Join(g.generator.GetFixtureDir(), "update_client_happy_path.json")
	g.generator.RequireFileExists(happyPathFile)

	validHex := g.extractHexFromHappyPathFixture(happyPathFile)

	// Create a header with future timestamp (beyond max clock drift)
	futureHex := g.createFutureTimestampHeader(validHex, int64(tmClientState.MaxClockDrift.Seconds()))

	futureUpdateMessage := map[string]interface{}{
		"client_message_hex": futureHex,
		"type_url":           "/ibc.lightclients.tendermint.v1.Header",
		"trusted_height":     tmClientState.LatestHeight.RevisionHeight,
		"new_height":         tmClientState.LatestHeight.RevisionHeight + 1,
		"metadata":           g.generator.CreateMetadata("Future timestamp - beyond max clock drift"),
	}

	unifiedFixture := map[string]interface{}{
		"scenario":                "future_timestamp",
		"client_state":            clientStateMap,
		"trusted_consensus_state": consensusStateMap,
		"update_client_message":   futureUpdateMessage,
		"metadata":                g.generator.CreateUnifiedMetadata("future_timestamp", tmClientState.ChainId),
	}

	filename := filepath.Join(g.generator.GetFixtureDir(), "update_client_future_timestamp.json")
	g.generator.SaveJsonFixture(filename, unifiedFixture)
	g.generator.LogInfof("💾 Future timestamp scenario fixture saved: %s", filename)
}

// generateWrongTrustedHeightScenario creates a fixture referencing wrong trusted height
func (g *UpdateClientFixtureGenerator) generateWrongTrustedHeightScenario(ctx context.Context, chainA *cosmos.CosmosChain) {
	g.generator.LogInfo("🔧 Generating wrong trusted height scenario")

	tmClientState := g.generator.QueryTendermintClientState(ctx, chainA)
	clientStateMap := g.generator.ConvertClientStateToFixtureFormat(tmClientState, chainA.Config().ChainID)

	tmConsensusState := g.generator.QueryTendermintConsensusState(ctx, chainA)
	consensusStateMap := g.generator.ConvertConsensusStateToFixtureFormat(tmConsensusState, chainA.Config().ChainID)

	happyPathFile := filepath.Join(g.generator.GetFixtureDir(), "update_client_happy_path.json")
	g.generator.RequireFileExists(happyPathFile)

	validHex := g.extractHexFromHappyPathFixture(happyPathFile)

	// Use the valid header but with wrong trusted height in metadata
	wrongHeightUpdateMessage := map[string]interface{}{
		"client_message_hex": validHex,
		"type_url":           "/ibc.lightclients.tendermint.v1.Header",
		"trusted_height":     tmClientState.LatestHeight.RevisionHeight + 100, // Wrong height
		"new_height":         tmClientState.LatestHeight.RevisionHeight + 1,
		"metadata":           g.generator.CreateMetadata("Wrong trusted height - references non-existent consensus state"),
	}

	unifiedFixture := map[string]interface{}{
		"scenario":                "wrong_trusted_height",
		"client_state":            clientStateMap,
		"trusted_consensus_state": consensusStateMap,
		"update_client_message":   wrongHeightUpdateMessage,
		"metadata":                g.generator.CreateUnifiedMetadata("wrong_trusted_height", tmClientState.ChainId),
	}

	filename := filepath.Join(g.generator.GetFixtureDir(), "update_client_wrong_trusted_height.json")
	g.generator.SaveJsonFixture(filename, unifiedFixture)
	g.generator.LogInfof("💾 Wrong trusted height scenario fixture saved: %s", filename)
}

// generateInvalidProtobufScenario creates a fixture with invalid protobuf bytes
func (g *UpdateClientFixtureGenerator) generateInvalidProtobufScenario() {
	g.generator.LogInfo("🔧 Generating invalid protobuf scenario")

	// Create completely invalid protobuf bytes
	invalidProtobuf := "FFFFFFFF" // Invalid protobuf that can't be decoded

	invalidUpdateMessage := map[string]interface{}{
		"client_message_hex": invalidProtobuf,
		"type_url":           "/ibc.lightclients.tendermint.v1.Header",
		"trusted_height":     19,
		"new_height":         20,
		"metadata":           g.generator.CreateMetadata("Invalid protobuf bytes - cannot be deserialized"),
	}

	// Use dummy client and consensus states
	dummyClientState := map[string]interface{}{
		"chain_id":                "test-chain",
		"trust_level_numerator":   1,
		"trust_level_denominator": 3,
		"trusting_period":         1209600,
		"unbonding_period":        1814400,
		"max_clock_drift":         10,
		"frozen_height":           0,
		"latest_height":           19,
		"metadata":                g.generator.CreateMetadata("Dummy client state for invalid protobuf test"),
	}

	dummyConsensusState := map[string]interface{}{
		"timestamp":            uint64(time.Now().Unix()),
		"root":                 hex.EncodeToString(make([]byte, 32)),
		"next_validators_hash": hex.EncodeToString(make([]byte, 32)),
		"metadata":             g.generator.CreateMetadata("Dummy consensus state for invalid protobuf test"),
	}

	unifiedFixture := map[string]interface{}{
		"scenario":                "invalid_protobuf",
		"client_state":            dummyClientState,
		"trusted_consensus_state": dummyConsensusState,
		"update_client_message":   invalidUpdateMessage,
		"metadata":                g.generator.CreateUnifiedMetadata("invalid_protobuf", "test-chain"),
	}

	filename := filepath.Join(g.generator.GetFixtureDir(), "update_client_invalid_protobuf.json")
	g.generator.SaveJsonFixture(filename, unifiedFixture)
	g.generator.LogInfof("💾 Invalid protobuf scenario fixture saved: %s", filename)
}

// Helper functions for modifying headers

func (g *UpdateClientFixtureGenerator) createExpiredHeader(validHex string, trustingPeriodSeconds int64) string {
	headerBytes, _ := hex.DecodeString(validHex)
	var header ibctmtypes.Header
	if err := proto.Unmarshal(headerBytes, &header); err != nil {
		g.generator.Fatalf("Failed to unmarshal header: %v", err)
	}

	// Set timestamp to be older than trusting period
	expiredTime := time.Now().Add(-time.Duration(trustingPeriodSeconds+3600) * time.Second) // Add 1 hour buffer
	header.Header.Time = expiredTime

	modifiedBytes, _ := proto.Marshal(&header)
	return hex.EncodeToString(modifiedBytes)
}

func (g *UpdateClientFixtureGenerator) createFutureTimestampHeader(validHex string, maxClockDriftSeconds int64) string {
	headerBytes, _ := hex.DecodeString(validHex)
	var header ibctmtypes.Header
	if err := proto.Unmarshal(headerBytes, &header); err != nil {
		g.generator.Fatalf("Failed to unmarshal header: %v", err)
	}

	// Set timestamp to be in the future beyond max clock drift
	futureTime := time.Now().Add(time.Duration(maxClockDriftSeconds+3600) * time.Second) // Add 1 hour buffer
	header.Header.Time = futureTime

	modifiedBytes, _ := proto.Marshal(&header)
	return hex.EncodeToString(modifiedBytes)
}
