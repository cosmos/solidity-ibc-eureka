package tendermint_light_client_fixtures

import (
	"context"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"time"

	"github.com/cosmos/gogoproto/proto"
	"github.com/stretchr/testify/suite"

	"github.com/cosmos/cosmos-sdk/codec/types"
	txtypes "github.com/cosmos/cosmos-sdk/types/tx"

	clienttypes "github.com/cosmos/ibc-go/v10/modules/core/02-client/types"
	ibctmtypes "github.com/cosmos/ibc-go/v10/modules/light-clients/07-tendermint"

	"github.com/strangelove-ventures/interchaintest/v8/chain/cosmos"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/e2esuite"
)

type UpdateClientFixtureGenerator struct {
	enabled    bool
	fixtureDir string
	suite      *suite.Suite
}

func NewUpdateClientFixtureGenerator(enabled bool, fixtureDir string, s *suite.Suite) *UpdateClientFixtureGenerator {
	return &UpdateClientFixtureGenerator{
		enabled:    enabled,
		fixtureDir: fixtureDir,
		suite:      s,
	}
}

func (g *UpdateClientFixtureGenerator) GenerateMultipleUpdateClientScenarios(
	ctx context.Context,
	chain *cosmos.CosmosChain,
	updateTxBodyBz []byte,
) {
	if !g.enabled {
		return
	}

	g.suite.T().Log("🔧 Generating multiple update client scenarios")

	msgUpdateClient := g.extractSingleUpdateClientMessageFromTransaction(updateTxBodyBz)
	clientId := msgUpdateClient.ClientId
	g.suite.T().Logf("📊 Found MsgUpdateClient for client: %s", clientId)

	g.generateHappyPathScenarioFromRealTransaction(ctx, chain, msgUpdateClient.ClientMessage, clientId)
	g.generateScenarioWithCorruptedSignature(ctx, chain, clientId)
	g.generateScenarioWithExpiredHeader(ctx, chain, clientId)
	g.generateScenarioWithFutureTimestamp(ctx, chain, clientId)
	g.generateScenarioWithNonExistentTrustedHeight(ctx, chain, clientId)

	g.suite.T().Log("✅ Multiple update client scenarios generated successfully")
}

func (g *UpdateClientFixtureGenerator) extractSingleUpdateClientMessageFromTransaction(txBodyBz []byte) *clienttypes.MsgUpdateClient {
	var txBody txtypes.TxBody
	err := proto.Unmarshal(txBodyBz, &txBody)
	g.suite.Require().NoError(err)
	g.suite.Require().Len(txBody.Messages, 1, "Expected exactly one message in update client tx")

	var msgUpdateClient clienttypes.MsgUpdateClient
	err = proto.Unmarshal(txBody.Messages[0].Value, &msgUpdateClient)
	g.suite.Require().NoError(err)
	g.suite.Require().NotNil(msgUpdateClient.ClientMessage)

	return &msgUpdateClient
}

func (g *UpdateClientFixtureGenerator) generateHappyPathScenarioFromRealTransaction(
	ctx context.Context,
	chain *cosmos.CosmosChain,
	clientMessage *types.Any,
	clientId string,
) {
	g.suite.T().Log("🔧 Generating happy path scenario")

	clientState := g.fetchAndFormatClientState(ctx, chain, clientId)
	consensusState := g.fetchAndFormatConsensusState(ctx, chain, clientId)
	updateMessage := g.formatClientMessageForFixture(clientMessage)

	fixture := g.createUpdateClientFixture(
		"happy_path",
		clientState,
		consensusState,
		updateMessage,
	)

	g.saveFixtureToFile(fixture, "update_client_happy_path.json")
}

func (g *UpdateClientFixtureGenerator) generateScenarioWithCorruptedSignature(
	ctx context.Context,
	chain *cosmos.CosmosChain,
	clientId string,
) {
	g.suite.T().Log("🔧 Generating malformed client message scenario")

	tmClientState := g.queryTendermintClientState(ctx, chain, clientId)
	clientState := g.fetchAndFormatClientState(ctx, chain, clientId)
	consensusState := g.fetchAndFormatConsensusState(ctx, chain, clientId)

	validHex := g.loadHexFromExistingHappyPathFixture()
	corruptedHex := g.corruptSignaturesWhilePreservingProtobufStructure(validHex)

	malformedMessage := g.createUpdateMessageWithCustomHex(
		corruptedHex,
		tmClientState.LatestHeight.RevisionHeight,
		"Intentionally malformed Tendermint header for unhappy path testing (signature corruption in valid protobuf structure)",
	)

	fixture := g.createUpdateClientFixture(
		"malformed_client_message",
		clientState,
		consensusState,
		malformedMessage,
	)

	g.saveFixtureToFile(fixture, "update_client_malformed_client_message.json")
}

func (g *UpdateClientFixtureGenerator) generateScenarioWithExpiredHeader(
	ctx context.Context,
	chain *cosmos.CosmosChain,
	clientId string,
) {
	g.suite.T().Log("🔧 Generating expired header scenario")

	tmClientState := g.queryTendermintClientState(ctx, chain, clientId)
	clientState := g.fetchAndFormatClientState(ctx, chain, clientId)
	consensusState := g.fetchAndFormatConsensusState(ctx, chain, clientId)

	validHex := g.loadHexFromExistingHappyPathFixture()
	expiredHex := g.modifyHeaderTimestampToPast(
		validHex,
		int64(tmClientState.TrustingPeriod.Seconds()),
	)

	expiredMessage := g.createUpdateMessageWithCustomHex(
		expiredHex,
		tmClientState.LatestHeight.RevisionHeight,
		"Expired header - timestamp beyond trusting period",
	)

	fixture := g.createUpdateClientFixture(
		"expired_header",
		clientState,
		consensusState,
		expiredMessage,
	)

	g.saveFixtureToFile(fixture, "update_client_expired_header.json")
}

func (g *UpdateClientFixtureGenerator) generateScenarioWithFutureTimestamp(
	ctx context.Context,
	chain *cosmos.CosmosChain,
	clientId string,
) {
	g.suite.T().Log("🔧 Generating future timestamp scenario")

	tmClientState := g.queryTendermintClientState(ctx, chain, clientId)
	clientState := g.fetchAndFormatClientState(ctx, chain, clientId)
	consensusState := g.fetchAndFormatConsensusState(ctx, chain, clientId)

	validHex := g.loadHexFromExistingHappyPathFixture()
	futureHex := g.modifyHeaderTimestampToFuture(
		validHex,
		int64(tmClientState.MaxClockDrift.Seconds()),
	)

	futureMessage := g.createUpdateMessageWithCustomHex(
		futureHex,
		tmClientState.LatestHeight.RevisionHeight,
		"Future timestamp - beyond max clock drift",
	)

	fixture := g.createUpdateClientFixture(
		"future_timestamp",
		clientState,
		consensusState,
		futureMessage,
	)

	g.saveFixtureToFile(fixture, "update_client_future_timestamp.json")
}

func (g *UpdateClientFixtureGenerator) generateScenarioWithNonExistentTrustedHeight(
	ctx context.Context,
	chain *cosmos.CosmosChain,
	clientId string,
) {
	g.suite.T().Log("🔧 Generating wrong trusted height scenario")

	tmClientState := g.queryTendermintClientState(ctx, chain, clientId)
	clientState := g.fetchAndFormatClientState(ctx, chain, clientId)
	consensusState := g.fetchAndFormatConsensusState(ctx, chain, clientId)
	validHex := g.loadHexFromExistingHappyPathFixture()

	latestHeight := tmClientState.LatestHeight.RevisionHeight
	nonExistentHeight := latestHeight + 100

	wrongHeightMessage := map[string]interface{}{
		"client_message_hex": validHex,
		"type_url":           "/ibc.lightclients.tendermint.v1.Header",
		"trusted_height":     nonExistentHeight,
		"new_height":         latestHeight + 1,
		"metadata":           g.createMetadata("Wrong trusted height - references non-existent consensus state"),
	}

	fixture := g.createUpdateClientFixture(
		"wrong_trusted_height",
		clientState,
		consensusState,
		wrongHeightMessage,
	)

	g.saveFixtureToFile(fixture, "update_client_wrong_trusted_height.json")
}

func (g *UpdateClientFixtureGenerator) fetchAndFormatClientState(
	ctx context.Context,
	chain *cosmos.CosmosChain,
	clientId string,
) string {
	tmClientState := g.queryTendermintClientState(ctx, chain, clientId)
	return g.convertClientStateToFixtureFormat(tmClientState)
}

func (g *UpdateClientFixtureGenerator) fetchAndFormatConsensusState(
	ctx context.Context,
	chain *cosmos.CosmosChain,
	clientId string,
) string {
	tmConsensusState := g.queryTendermintLatestConsensusState(ctx, chain, clientId)
	return g.convertConsensusStateToFixtureFormat(tmConsensusState)
}

func (g *UpdateClientFixtureGenerator) formatClientMessageForFixture(clientMessage *types.Any) map[string]interface{} {
	headerBytes := clientMessage.Value
	tmHeader := g.parseAndValidateTendermintHeader(headerBytes)

	return map[string]interface{}{
		"client_message_hex": hex.EncodeToString(headerBytes),
		"type_url":           clientMessage.TypeUrl,
		"trusted_height":     tmHeader.TrustedHeight.RevisionHeight,
		"new_height":         tmHeader.Header.Height,
		"metadata":           g.createMetadata("Protobuf-encoded Tendermint header for update client"),
	}
}

func (g *UpdateClientFixtureGenerator) parseAndValidateTendermintHeader(headerBytes []byte) *ibctmtypes.Header {
	var tmHeader ibctmtypes.Header
	err := proto.Unmarshal(headerBytes, &tmHeader)
	g.suite.Require().NoError(err, "Failed to parse header for height extraction - fixture generation cannot continue")

	trustedHeight := tmHeader.TrustedHeight.RevisionHeight
	newHeight := tmHeader.Header.Height

	g.suite.Require().Greater(newHeight, int64(0), "New height must be greater than 0")
	g.suite.Require().Greater(trustedHeight, uint64(0), "Trusted height must be greater than 0")
	g.suite.Require().Greater(newHeight, int64(trustedHeight), "New height must be greater than trusted height")

	return &tmHeader
}

func (g *UpdateClientFixtureGenerator) loadHexFromExistingHappyPathFixture() string {
	happyPathFile := filepath.Join(g.fixtureDir, "update_client_happy_path.json")
	g.suite.Require().FileExists(happyPathFile, "Happy path fixture must exist before generating malformed fixture")

	g.suite.T().Log("📖 Loading happy path fixture to create modified version")

	data, err := os.ReadFile(happyPathFile)
	g.suite.Require().NoError(err, "Failed to read happy path fixture")

	var fixture map[string]interface{}
	err = json.Unmarshal(data, &fixture)
	g.suite.Require().NoError(err, "Failed to parse happy path fixture JSON")

	updateMessage, ok := fixture["update_client_message"].(map[string]interface{})
	g.suite.Require().True(ok, "update_client_message not found in happy path fixture")

	hexString, ok := updateMessage["client_message_hex"].(string)
	g.suite.Require().True(ok, "client_message_hex not found in happy path fixture")

	return hexString
}

func (g *UpdateClientFixtureGenerator) corruptSignaturesWhilePreservingProtobufStructure(validHex string) string {
	headerBytes, err := hex.DecodeString(validHex)
	if err != nil {
		g.suite.T().Fatalf("Failed to decode valid header hex: %v", err)
	}

	var tmHeader ibctmtypes.Header
	err = proto.Unmarshal(headerBytes, &tmHeader)
	if err != nil {
		g.suite.T().Fatalf("Failed to parse header for corruption: %v", err)
	}

	corruptedHeader := tmHeader
	g.flipBytesInCommitSignatures(&corruptedHeader)
	g.flipBytesInBlockHash(&corruptedHeader)

	corruptedBytes, err := proto.Marshal(&corruptedHeader)
	if err != nil {
		g.suite.T().Fatalf("Failed to marshal corrupted header: %v", err)
	}

	g.ensureHeaderStillParseable(corruptedBytes)
	g.suite.T().Log("🔧 Header corrupted successfully - still deserializable but signatures are invalid")

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
		g.suite.T().Logf("🔧 Corrupted signature byte at position %d in first commit signature", sigPos)
	}
}

func (g *UpdateClientFixtureGenerator) flipBytesInBlockHash(header *ibctmtypes.Header) {
	if header.Commit == nil || len(header.Commit.BlockID.Hash) == 0 {
		return
	}

	hashPos := len(header.Commit.BlockID.Hash) / 2
	header.Commit.BlockID.Hash[hashPos] ^= 0xFF
	g.suite.T().Logf("🔧 Corrupted block hash byte at position %d", hashPos)
}

func (g *UpdateClientFixtureGenerator) ensureHeaderStillParseable(headerBytes []byte) {
	var testHeader ibctmtypes.Header
	err := proto.Unmarshal(headerBytes, &testHeader)
	if err != nil {
		g.suite.T().Fatalf("Corrupted header failed to parse - corruption was too aggressive: %v", err)
	}
}

func (g *UpdateClientFixtureGenerator) modifyHeaderTimestampToPast(validHex string, trustingPeriodSeconds int64) string {
	headerBytes, _ := hex.DecodeString(validHex)
	var header ibctmtypes.Header
	if err := proto.Unmarshal(headerBytes, &header); err != nil {
		g.suite.T().Fatalf("Failed to unmarshal header: %v", err)
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
		g.suite.T().Fatalf("Failed to unmarshal header: %v", err)
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
		"metadata":           g.createMetadata(description),
	}
}

func (g *UpdateClientFixtureGenerator) createUpdateClientFixture(
	scenario string,
	clientStateHex string,
	consensusStateHex string,
	updateMessage map[string]interface{},
) map[string]interface{} {
	return map[string]interface{}{
		"client_state_hex":      clientStateHex,
		"consensus_state_hex":   consensusStateHex,
		"update_client_message": updateMessage,
		"metadata":              g.createMetadata(fmt.Sprintf("Tendermint light client fixture for scenario: %s", scenario)),
	}
}

func (g *UpdateClientFixtureGenerator) saveFixtureToFile(fixture map[string]interface{}, filename string) {
	fullPath := filepath.Join(g.fixtureDir, filename)
	g.saveJsonFixture(fullPath, fixture)
	g.suite.T().Logf("💾 Fixture saved: %s", fullPath)
}

func (g *UpdateClientFixtureGenerator) saveJsonFixture(filename string, data interface{}) {
	jsonData, err := json.MarshalIndent(data, "", "  ")
	g.suite.Require().NoError(err)

	err = os.WriteFile(filename, jsonData, 0o600)
	g.suite.Require().NoError(err)
}

func (g *UpdateClientFixtureGenerator) queryTendermintClientState(ctx context.Context, chainA *cosmos.CosmosChain, clientId string) *ibctmtypes.ClientState {
	resp, err := e2esuite.GRPCQuery[clienttypes.QueryClientStateResponse](ctx, chainA, &clienttypes.QueryClientStateRequest{
		ClientId: clientId,
	})
	g.suite.Require().NoError(err)
	g.suite.Require().NotNil(resp.ClientState)

	var tmClientState ibctmtypes.ClientState
	err = proto.Unmarshal(resp.ClientState.Value, &tmClientState)
	g.suite.Require().NoError(err)

	return &tmClientState
}

func (g *UpdateClientFixtureGenerator) queryTendermintLatestConsensusState(ctx context.Context, chainA *cosmos.CosmosChain, clientId string) *ibctmtypes.ConsensusState {
	resp, err := e2esuite.GRPCQuery[clienttypes.QueryConsensusStateResponse](ctx, chainA, &clienttypes.QueryConsensusStateRequest{
		ClientId:     clientId,
		LatestHeight: true,
	})
	g.suite.Require().NoError(err)
	g.suite.Require().NotNil(resp.ConsensusState)

	var tmConsensusState ibctmtypes.ConsensusState
	err = proto.Unmarshal(resp.ConsensusState.Value, &tmConsensusState)
	g.suite.Require().NoError(err)

	return &tmConsensusState
}

func (g *UpdateClientFixtureGenerator) convertClientStateToFixtureFormat(tmClientState *ibctmtypes.ClientState) string {
	clientStateBytes, err := proto.Marshal(tmClientState)
	g.suite.Require().NoError(err)

	return hex.EncodeToString(clientStateBytes)
}

func (g *UpdateClientFixtureGenerator) convertConsensusStateToFixtureFormat(tmConsensusState *ibctmtypes.ConsensusState) string {
	consensusStateBytes, err := proto.Marshal(tmConsensusState)
	g.suite.Require().NoError(err)

	return hex.EncodeToString(consensusStateBytes)
}

func (g *UpdateClientFixtureGenerator) createMetadata(description string) map[string]interface{} {
	return map[string]interface{}{
		"generated_at": time.Now().UTC().Format(time.RFC3339),
		"source":       "local_cosmos_chain",
		"description":  description,
	}
}
