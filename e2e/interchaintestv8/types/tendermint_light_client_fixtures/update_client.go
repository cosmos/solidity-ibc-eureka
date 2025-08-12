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

type UpdateClientFixtureGenerator struct {
	generator FixtureGeneratorInterface
}

func NewUpdateClientFixtureGenerator(generator FixtureGeneratorInterface) *UpdateClientFixtureGenerator {
	return &UpdateClientFixtureGenerator{
		generator: generator,
	}
}

func (g *UpdateClientFixtureGenerator) GenerateMultipleUpdateClientScenarios(
	ctx context.Context,
	chainA *cosmos.CosmosChain,
	updateTxBodyBz []byte,
) {
	if !g.generator.IsEnabled() {
		return
	}

	g.generator.LogInfo("🔧 Generating multiple update client scenarios")

	msgUpdateClient := g.extractSingleUpdateClientMessageFromTransaction(updateTxBodyBz)
	clientId := msgUpdateClient.ClientId
	g.generator.LogInfof("📊 Found MsgUpdateClient for client: %s", clientId)

	g.generateHappyPathScenarioFromRealTransaction(ctx, chainA, msgUpdateClient.ClientMessage, clientId)
	g.generateScenarioWithCorruptedSignature(ctx, chainA, clientId)
	g.generateScenarioWithExpiredHeader(ctx, chainA, clientId)
	g.generateScenarioWithFutureTimestamp(ctx, chainA, clientId)
	g.generateScenarioWithNonExistentTrustedHeight(ctx, chainA, clientId)
	g.generateScenarioWithUnparseableProtobuf()

	g.generator.LogInfo("✅ Multiple update client scenarios generated successfully")
}

func (g *UpdateClientFixtureGenerator) extractSingleUpdateClientMessageFromTransaction(txBodyBz []byte) *clienttypes.MsgUpdateClient {
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

func (g *UpdateClientFixtureGenerator) generateHappyPathScenarioFromRealTransaction(
	ctx context.Context,
	chainA *cosmos.CosmosChain,
	clientMessage *types.Any,
	clientId string,
) {
	g.generator.LogInfo("🔧 Generating happy path scenario")

	clientState := g.fetchAndFormatClientState(ctx, chainA, clientId)
	consensusState := g.fetchAndFormatConsensusState(ctx, chainA, clientId)
	updateMessage := g.formatClientMessageForFixture(clientMessage)

	fixture := g.createUpdateClientFixture(
		"happy_path",
		clientState,
		consensusState,
		updateMessage,
		chainA.Config().ChainID,
	)

	g.saveFixtureToFile(fixture, "update_client_happy_path.json")
}

func (g *UpdateClientFixtureGenerator) generateScenarioWithCorruptedSignature(
	ctx context.Context,
	chainA *cosmos.CosmosChain,
	clientId string,
) {
	g.generator.LogInfo("🔧 Generating malformed client message scenario")

	clientState := g.fetchAndFormatClientState(ctx, chainA, clientId)
	consensusState := g.fetchAndFormatConsensusState(ctx, chainA, clientId)

	validHex := g.loadHexFromExistingHappyPathFixture()
	corruptedHex := g.corruptSignaturesWhilePreservingProtobufStructure(validHex)

	malformedMessage := g.createUpdateMessageWithCustomHex(
		corruptedHex,
		clientState["latest_height"].(uint64),
		"Intentionally malformed Tendermint header for unhappy path testing (signature corruption in valid protobuf structure)",
	)

	fixture := g.createUpdateClientFixture(
		"malformed_client_message",
		clientState,
		consensusState,
		malformedMessage,
		chainA.Config().ChainID,
	)

	g.saveFixtureToFile(fixture, "update_client_malformed_client_message.json")
}

func (g *UpdateClientFixtureGenerator) generateScenarioWithExpiredHeader(
	ctx context.Context,
	chainA *cosmos.CosmosChain,
	clientId string,
) {
	g.generator.LogInfo("🔧 Generating expired header scenario")

	tmClientState := g.generator.QueryTendermintClientState(ctx, chainA, clientId)
	clientState := g.fetchAndFormatClientState(ctx, chainA, clientId)
	consensusState := g.fetchAndFormatConsensusState(ctx, chainA, clientId)

	validHex := g.loadHexFromExistingHappyPathFixture()
	expiredHex := g.modifyHeaderTimestampToPast(
		validHex,
		int64(tmClientState.TrustingPeriod.Seconds()),
	)

	expiredMessage := g.createUpdateMessageWithCustomHex(
		expiredHex,
		clientState["latest_height"].(uint64),
		"Expired header - timestamp beyond trusting period",
	)

	fixture := g.createUpdateClientFixture(
		"expired_header",
		clientState,
		consensusState,
		expiredMessage,
		chainA.Config().ChainID,
	)

	g.saveFixtureToFile(fixture, "update_client_expired_header.json")
}

func (g *UpdateClientFixtureGenerator) generateScenarioWithFutureTimestamp(
	ctx context.Context,
	chainA *cosmos.CosmosChain,
	clientId string,
) {
	g.generator.LogInfo("🔧 Generating future timestamp scenario")

	tmClientState := g.generator.QueryTendermintClientState(ctx, chainA, clientId)
	clientState := g.fetchAndFormatClientState(ctx, chainA, clientId)
	consensusState := g.fetchAndFormatConsensusState(ctx, chainA, clientId)

	validHex := g.loadHexFromExistingHappyPathFixture()
	futureHex := g.modifyHeaderTimestampToFuture(
		validHex,
		int64(tmClientState.MaxClockDrift.Seconds()),
	)

	futureMessage := g.createUpdateMessageWithCustomHex(
		futureHex,
		clientState["latest_height"].(uint64),
		"Future timestamp - beyond max clock drift",
	)

	fixture := g.createUpdateClientFixture(
		"future_timestamp",
		clientState,
		consensusState,
		futureMessage,
		chainA.Config().ChainID,
	)

	g.saveFixtureToFile(fixture, "update_client_future_timestamp.json")
}

func (g *UpdateClientFixtureGenerator) generateScenarioWithNonExistentTrustedHeight(
	ctx context.Context,
	chainA *cosmos.CosmosChain,
	clientId string,
) {
	g.generator.LogInfo("🔧 Generating wrong trusted height scenario")

	clientState := g.fetchAndFormatClientState(ctx, chainA, clientId)
	consensusState := g.fetchAndFormatConsensusState(ctx, chainA, clientId)
	validHex := g.loadHexFromExistingHappyPathFixture()

	latestHeight := clientState["latest_height"].(uint64)
	nonExistentHeight := latestHeight + 100

	wrongHeightMessage := map[string]interface{}{
		"client_message_hex": validHex,
		"type_url":           "/ibc.lightclients.tendermint.v1.Header",
		"trusted_height":     nonExistentHeight,
		"new_height":         latestHeight + 1,
		"metadata":           g.generator.CreateMetadata("Wrong trusted height - references non-existent consensus state"),
	}

	fixture := g.createUpdateClientFixture(
		"wrong_trusted_height",
		clientState,
		consensusState,
		wrongHeightMessage,
		chainA.Config().ChainID,
	)

	g.saveFixtureToFile(fixture, "update_client_wrong_trusted_height.json")
}

func (g *UpdateClientFixtureGenerator) generateScenarioWithUnparseableProtobuf() {
	g.generator.LogInfo("🔧 Generating invalid protobuf scenario")

	invalidProtobufBytes := "FFFFFFFF"

	invalidMessage := map[string]interface{}{
		"client_message_hex": invalidProtobufBytes,
		"type_url":           "/ibc.lightclients.tendermint.v1.Header",
		"trusted_height":     19,
		"new_height":         20,
		"metadata":           g.generator.CreateMetadata("Invalid protobuf bytes - cannot be deserialized"),
	}

	clientState := g.createDummyClientStateForTesting()
	consensusState := g.createDummyConsensusStateForTesting()

	fixture := g.createUpdateClientFixture(
		"invalid_protobuf",
		clientState,
		consensusState,
		invalidMessage,
		"test-chain",
	)

	g.saveFixtureToFile(fixture, "update_client_invalid_protobuf.json")
}

func (g *UpdateClientFixtureGenerator) fetchAndFormatClientState(
	ctx context.Context,
	chainA *cosmos.CosmosChain,
	clientId string,
) map[string]interface{} {
	tmClientState := g.generator.QueryTendermintClientState(ctx, chainA, clientId)
	return g.generator.ConvertClientStateToFixtureFormat(tmClientState, chainA.Config().ChainID)
}

func (g *UpdateClientFixtureGenerator) fetchAndFormatConsensusState(
	ctx context.Context,
	chainA *cosmos.CosmosChain,
	clientId string,
) map[string]interface{} {
	tmConsensusState := g.generator.QueryTendermintConsensusState(ctx, chainA, clientId)
	return g.generator.ConvertConsensusStateToFixtureFormat(tmConsensusState, chainA.Config().ChainID)
}

func (g *UpdateClientFixtureGenerator) formatClientMessageForFixture(clientMessage *types.Any) map[string]interface{} {
	headerBytes := clientMessage.Value
	tmHeader := g.parseAndValidateTendermintHeader(headerBytes)

	return map[string]interface{}{
		"client_message_hex": hex.EncodeToString(headerBytes),
		"type_url":           clientMessage.TypeUrl,
		"trusted_height":     tmHeader.TrustedHeight.RevisionHeight,
		"new_height":         tmHeader.Header.Height,
		"metadata":           g.generator.CreateMetadata("Protobuf-encoded Tendermint header for update client"),
	}
}

func (g *UpdateClientFixtureGenerator) parseAndValidateTendermintHeader(headerBytes []byte) *ibctmtypes.Header {
	var tmHeader ibctmtypes.Header
	err := proto.Unmarshal(headerBytes, &tmHeader)
	g.generator.RequireNoError(err, "Failed to parse header for height extraction - fixture generation cannot continue")

	trustedHeight := tmHeader.TrustedHeight.RevisionHeight
	newHeight := tmHeader.Header.Height

	g.generator.RequireGreater(newHeight, int64(0), "New height must be greater than 0")
	g.generator.RequireGreater(trustedHeight, uint64(0), "Trusted height must be greater than 0")
	g.generator.RequireGreater(newHeight, int64(trustedHeight), "New height must be greater than trusted height")

	return &tmHeader
}

func (g *UpdateClientFixtureGenerator) loadHexFromExistingHappyPathFixture() string {
	happyPathFile := filepath.Join(g.generator.GetFixtureDir(), "update_client_happy_path.json")
	g.generator.RequireFileExists(happyPathFile, "Happy path fixture must exist before generating malformed fixture")

	g.generator.LogInfo("📖 Loading happy path fixture to create modified version")

	data, err := os.ReadFile(happyPathFile)
	g.generator.RequireNoError(err, "Failed to read happy path fixture")

	var fixture map[string]interface{}
	err = json.Unmarshal(data, &fixture)
	g.generator.RequireNoError(err, "Failed to parse happy path fixture JSON")

	updateMessage, ok := fixture["update_client_message"].(map[string]interface{})
	g.generator.RequireTrue(ok, "update_client_message not found in happy path fixture")

	hexString, ok := updateMessage["client_message_hex"].(string)
	g.generator.RequireTrue(ok, "client_message_hex not found in happy path fixture")

	return hexString
}

func (g *UpdateClientFixtureGenerator) corruptSignaturesWhilePreservingProtobufStructure(validHex string) string {
	headerBytes, err := hex.DecodeString(validHex)
	if err != nil {
		g.generator.Fatalf("Failed to decode valid header hex: %v", err)
	}

	var tmHeader ibctmtypes.Header
	err = proto.Unmarshal(headerBytes, &tmHeader)
	if err != nil {
		g.generator.Fatalf("Failed to parse header for corruption: %v", err)
	}

	corruptedHeader := tmHeader
	g.flipBytesInCommitSignatures(&corruptedHeader)
	g.flipBytesInBlockHash(&corruptedHeader)

	corruptedBytes, err := proto.Marshal(&corruptedHeader)
	if err != nil {
		g.generator.Fatalf("Failed to marshal corrupted header: %v", err)
	}

	g.ensureHeaderStillParseable(corruptedBytes)
	g.generator.LogInfo("🔧 Header corrupted successfully - still deserializable but signatures are invalid")

	return hex.EncodeToString(corruptedBytes)
}

func (g *UpdateClientFixtureGenerator) flipBytesInCommitSignatures(header *ibctmtypes.Header) {
	if header.SignedHeader == nil || header.Commit == nil {
		return
	}

	commit := header.Commit
	if len(commit.Signatures) > 0 && len(commit.Signatures[0].Signature) > 10 {
		sigPos := len(commit.Signatures[0].Signature) / 2
		commit.Signatures[0].Signature[sigPos] ^= 0xFF
		g.generator.LogInfof("🔧 Corrupted signature byte at position %d in first commit signature", sigPos)
	}
}

func (g *UpdateClientFixtureGenerator) flipBytesInBlockHash(header *ibctmtypes.Header) {
	if header.Commit == nil || len(header.Commit.BlockID.Hash) == 0 {
		return
	}

	hashPos := len(header.Commit.BlockID.Hash) / 2
	header.Commit.BlockID.Hash[hashPos] ^= 0xFF
	g.generator.LogInfof("🔧 Corrupted block hash byte at position %d", hashPos)
}

func (g *UpdateClientFixtureGenerator) ensureHeaderStillParseable(headerBytes []byte) {
	var testHeader ibctmtypes.Header
	err := proto.Unmarshal(headerBytes, &testHeader)
	if err != nil {
		g.generator.Fatalf("Corrupted header failed to parse - corruption was too aggressive: %v", err)
	}
}

func (g *UpdateClientFixtureGenerator) modifyHeaderTimestampToPast(validHex string, trustingPeriodSeconds int64) string {
	headerBytes, _ := hex.DecodeString(validHex)
	var header ibctmtypes.Header
	if err := proto.Unmarshal(headerBytes, &header); err != nil {
		g.generator.Fatalf("Failed to unmarshal header: %v", err)
	}

	oneHourBuffer := int64(3600)
	expiredTime := time.Now().Add(-time.Duration(trustingPeriodSeconds+oneHourBuffer) * time.Second)
	header.Header.Time = expiredTime

	modifiedBytes, _ := proto.Marshal(&header)
	return hex.EncodeToString(modifiedBytes)
}

func (g *UpdateClientFixtureGenerator) modifyHeaderTimestampToFuture(validHex string, maxClockDriftSeconds int64) string {
	headerBytes, _ := hex.DecodeString(validHex)
	var header ibctmtypes.Header
	if err := proto.Unmarshal(headerBytes, &header); err != nil {
		g.generator.Fatalf("Failed to unmarshal header: %v", err)
	}

	oneHourBuffer := int64(3600)
	futureTime := time.Now().Add(time.Duration(maxClockDriftSeconds+oneHourBuffer) * time.Second)
	header.Header.Time = futureTime

	modifiedBytes, _ := proto.Marshal(&header)
	return hex.EncodeToString(modifiedBytes)
}

func (g *UpdateClientFixtureGenerator) createUpdateMessageWithCustomHex(
	hexString string,
	trustedHeight uint64,
	description string,
) map[string]interface{} {
	return map[string]interface{}{
		"client_message_hex": hexString,
		"type_url":           "/ibc.lightclients.tendermint.v1.Header",
		"trusted_height":     trustedHeight,
		"new_height":         trustedHeight + 1,
		"metadata":           g.generator.CreateMetadata(description),
	}
}

func (g *UpdateClientFixtureGenerator) createDummyClientStateForTesting() map[string]interface{} {
	return map[string]interface{}{
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
}

func (g *UpdateClientFixtureGenerator) createDummyConsensusStateForTesting() map[string]interface{} {
	return map[string]interface{}{
		"timestamp":            uint64(time.Now().Unix()),
		"root":                 hex.EncodeToString(make([]byte, 32)),
		"next_validators_hash": hex.EncodeToString(make([]byte, 32)),
		"metadata":             g.generator.CreateMetadata("Dummy consensus state for invalid protobuf test"),
	}
}

func (g *UpdateClientFixtureGenerator) createUpdateClientFixture(
	scenario string,
	clientState map[string]interface{},
	consensusState map[string]interface{},
	updateMessage map[string]interface{},
	chainID string,
) map[string]interface{} {
	return map[string]interface{}{
		"scenario":                scenario,
		"client_state":            clientState,
		"trusted_consensus_state": consensusState,
		"update_client_message":   updateMessage,
		"metadata":                g.generator.CreateUnifiedMetadata(scenario, chainID),
	}
}

func (g *UpdateClientFixtureGenerator) saveFixtureToFile(fixture map[string]interface{}, filename string) {
	fullPath := filepath.Join(g.generator.GetFixtureDir(), filename)
	g.generator.SaveJsonFixture(fullPath, fixture)
	g.generator.LogInfof("💾 Fixture saved: %s", fullPath)
}
