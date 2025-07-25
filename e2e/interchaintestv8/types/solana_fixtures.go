package types

import (
	"context"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/cosmos/gogoproto/proto"
	"github.com/stretchr/testify/suite"

	"github.com/cosmos/cosmos-sdk/codec/types"
	txtypes "github.com/cosmos/cosmos-sdk/types/tx"
	clienttypes "github.com/cosmos/ibc-go/v10/modules/core/02-client/types"
	commitmenttypes "github.com/cosmos/ibc-go/v10/modules/core/23-commitment/types"
	ibctmtypes "github.com/cosmos/ibc-go/v10/modules/light-clients/07-tendermint"
	ibctesting "github.com/cosmos/ibc-go/v10/testing"
	ics23 "github.com/cosmos/ics23/go"

	abci "github.com/cometbft/cometbft/abci/types"
	tmcrypto "github.com/cometbft/cometbft/proto/tendermint/crypto"
	"github.com/strangelove-ventures/interchaintest/v8/chain/cosmos"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/e2esuite"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
)

type SolanaFixtureGenerator struct {
	Enabled    bool
	FixtureDir string
	suite      *suite.Suite
}

func NewSolanaFixtureGenerator(s *suite.Suite) *SolanaFixtureGenerator {
	generator := &SolanaFixtureGenerator{
		Enabled: os.Getenv("GENERATE_SOLANA_FIXTURES") == "true",
		suite:   s,
	}

	if generator.Enabled {
		absPath, err := filepath.Abs(filepath.Join("../..", testvalues.SolanaFixturesDir))
		if err != nil {
			s.T().Fatalf("Failed to get absolute path for fixtures: %v", err)
		}
		generator.FixtureDir = absPath

		if err := os.MkdirAll(generator.FixtureDir, 0755); err != nil {
			s.T().Fatalf("Failed to create Solana fixture directory: %v", err)
		}
		s.T().Logf("📁 Solana fixtures will be saved to: %s", generator.FixtureDir)
	}

	return generator
}

func (g *SolanaFixtureGenerator) GenerateFixturesFromUpdateTx(ctx context.Context, updateTxBodyBz []byte, chainA *cosmos.CosmosChain) {
	if !g.Enabled {
		return
	}

	g.suite.T().Log("🔍 Parsing update client transaction")

	msgUpdateClient := g.extractUpdateClientMessage(updateTxBodyBz)
	g.suite.T().Logf("📊 Found MsgUpdateClient for client: %s", msgUpdateClient.ClientId)

	g.generateClientStateFixture(ctx, chainA)
	g.generateConsensusStateFixture(ctx, chainA)
	g.generateUpdateClientMessageFixture(msgUpdateClient.ClientMessage)

	g.suite.T().Log("✅ Solana fixtures generated successfully")
}

func (g *SolanaFixtureGenerator) extractUpdateClientMessage(txBodyBz []byte) *clienttypes.MsgUpdateClient {
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

func (g *SolanaFixtureGenerator) generateClientStateFixture(ctx context.Context, chainA *cosmos.CosmosChain) {
	g.suite.T().Log("🔧 Generating ClientState fixture")

	tmClientState := g.queryTendermintClientState(ctx, chainA)
	solanaClientState := g.convertClientStateToSolanaFormat(tmClientState, chainA.Config().ChainID)

	filename := filepath.Join(g.FixtureDir, "client_state.json")
	g.saveJsonFixture(filename, solanaClientState)
	g.suite.T().Logf("💾 Client state fixture saved: %s", filename)
}

func (g *SolanaFixtureGenerator) queryTendermintClientState(ctx context.Context, chainA *cosmos.CosmosChain) *ibctmtypes.ClientState {
	resp, err := e2esuite.GRPCQuery[clienttypes.QueryClientStateResponse](ctx, chainA, &clienttypes.QueryClientStateRequest{
		ClientId: ibctesting.FirstClientID,
	})
	g.suite.Require().NoError(err)
	g.suite.Require().NotNil(resp.ClientState)

	var tmClientState ibctmtypes.ClientState
	err = proto.Unmarshal(resp.ClientState.Value, &tmClientState)
	g.suite.Require().NoError(err)

	return &tmClientState
}

func (g *SolanaFixtureGenerator) convertClientStateToSolanaFormat(tmClientState *ibctmtypes.ClientState, chainID string) map[string]interface{} {
	return map[string]interface{}{
		"chain_id":                tmClientState.ChainId,
		"trust_level_numerator":   tmClientState.TrustLevel.Numerator,
		"trust_level_denominator": tmClientState.TrustLevel.Denominator,
		"trusting_period":         tmClientState.TrustingPeriod.Seconds(),
		"unbonding_period":        tmClientState.UnbondingPeriod.Seconds(),
		"max_clock_drift":         tmClientState.MaxClockDrift.Seconds(),
		"frozen_height":           tmClientState.FrozenHeight.RevisionHeight,
		"latest_height":           tmClientState.LatestHeight.RevisionHeight,
		"metadata":                g.createMetadata(fmt.Sprintf("Client state for %s captured from %s", tmClientState.ChainId, chainID)),
	}
}

func (g *SolanaFixtureGenerator) generateConsensusStateFixture(ctx context.Context, chainA *cosmos.CosmosChain) {
	g.suite.T().Log("🔧 Generating ConsensusState fixture")

	tmConsensusState := g.queryTendermintConsensusState(ctx, chainA)
	solanaConsensusState := g.convertConsensusStateToSolanaFormat(tmConsensusState, chainA.Config().ChainID)

	filename := filepath.Join(g.FixtureDir, "consensus_state.json")
	g.saveJsonFixture(filename, solanaConsensusState)
	g.suite.T().Logf("💾 Consensus state fixture saved: %s", filename)
}

func (g *SolanaFixtureGenerator) queryTendermintConsensusState(ctx context.Context, chainA *cosmos.CosmosChain) *ibctmtypes.ConsensusState {
	resp, err := e2esuite.GRPCQuery[clienttypes.QueryConsensusStateResponse](ctx, chainA, &clienttypes.QueryConsensusStateRequest{
		ClientId:       ibctesting.FirstClientID,
		RevisionNumber: 1,
		RevisionHeight: 1,
		LatestHeight:   true,
	})
	g.suite.Require().NoError(err)
	g.suite.Require().NotNil(resp.ConsensusState)

	var tmConsensusState ibctmtypes.ConsensusState
	err = proto.Unmarshal(resp.ConsensusState.Value, &tmConsensusState)
	g.suite.Require().NoError(err)

	return &tmConsensusState
}

func (g *SolanaFixtureGenerator) convertConsensusStateToSolanaFormat(tmConsensusState *ibctmtypes.ConsensusState, chainID string) map[string]interface{} {
	return map[string]interface{}{
		"timestamp":            tmConsensusState.Timestamp.UnixNano(),
		"root":                 hex.EncodeToString(tmConsensusState.Root.GetHash()),
		"next_validators_hash": hex.EncodeToString(tmConsensusState.NextValidatorsHash),
		"metadata":             g.createMetadata(fmt.Sprintf("Consensus state captured from %s", chainID)),
	}
}

func (g *SolanaFixtureGenerator) generateUpdateClientMessageFixture(clientMessage *types.Any) {
	g.suite.T().Log("🔧 Generating UpdateClientMessage fixture")

	headerBytes := clientMessage.Value
	updateClientMessage := map[string]interface{}{
		"client_message_hex":    hex.EncodeToString(headerBytes),
		"client_message_base64": hex.EncodeToString(headerBytes),
		"client_message_bytes":  headerBytes,
		"type_url":              clientMessage.TypeUrl,
		"metadata":              g.createMetadata("Protobuf-encoded Tendermint header for update client"),
	}

	filename := filepath.Join(g.FixtureDir, "update_client_message.json")
	g.saveJsonFixture(filename, updateClientMessage)
	g.suite.T().Logf("💾 Update client message fixture saved: %s", filename)
}

func (g *SolanaFixtureGenerator) GenerateMembershipFixtures(ctx context.Context, chainA *cosmos.CosmosChain, keyPaths []string) {
	if !g.Enabled {
		return
	}

	g.suite.T().Log("🔧 Generating Membership fixtures for Solana")

	clientState := g.getCurrentClientState(ctx, chainA)
	consensusState := g.getCurrentConsensusState(ctx, chainA)

	for i, keyPath := range keyPaths {
		g.generateMembershipAndNonMembershipFixtures(ctx, chainA, keyPath, clientState, consensusState, i)
	}

	g.suite.T().Log("✅ Solana membership fixtures generated successfully")
}

func (g *SolanaFixtureGenerator) generateMembershipAndNonMembershipFixtures(
	ctx context.Context,
	chainA *cosmos.CosmosChain,
	keyPath string,
	clientState, consensusState map[string]interface{},
	index int,
) {
	g.suite.T().Logf("📝 Generating membership fixture for path: %s", keyPath)

	membershipFixture := g.generateMembershipFixture(ctx, chainA, keyPath, clientState, consensusState, true)
	filename := filepath.Join(g.FixtureDir, fmt.Sprintf("membership_%d.json", index))
	g.saveJsonFixture(filename, membershipFixture)
	g.suite.T().Logf("💾 Membership fixture saved: %s", filename)

	nonMembershipPath := keyPath + "_nonexistent"
	nonMembershipFixture := g.generateMembershipFixture(ctx, chainA, nonMembershipPath, clientState, consensusState, false)
	filename = filepath.Join(g.FixtureDir, fmt.Sprintf("non_membership_%d.json", index))
	g.saveJsonFixture(filename, nonMembershipFixture)
	g.suite.T().Logf("💾 Non-membership fixture saved: %s", filename)
}

func (g *SolanaFixtureGenerator) generateMembershipFixture(
	ctx context.Context,
	chainA *cosmos.CosmosChain,
	keyPath string,
	clientState map[string]interface{},
	consensusState map[string]interface{},
	expectExists bool,
) map[string]interface{} {
	height := g.determineProofHeight(ctx, chainA, clientState)

	updatedClientState := g.updateClientStateHeight(clientState, height)
	updatedConsensusState := g.getBlockchainConsensusStateAtHeight(ctx, chainA, height)

	value, proofBytes := g.queryProofForPath(ctx, chainA, keyPath, int64(height), expectExists)

	membershipMsg := g.createMembershipMessage(height, proofBytes, keyPath, value)

	g.logMembershipFixtureDetails(keyPath, expectExists, height, value, proofBytes)

	return map[string]interface{}{
		"client_state":    updatedClientState,
		"consensus_state": updatedConsensusState,
		"membership_msg":  membershipMsg,
		"expected_result": "success",
		"metadata":        g.createMembershipMetadata(keyPath, expectExists, height),
	}
}

func (g *SolanaFixtureGenerator) determineProofHeight(ctx context.Context, chainA *cosmos.CosmosChain, clientState map[string]interface{}) uint64 {
	height := clientState["latest_height"].(uint64)
	currentHeight, err := chainA.Height(ctx)
	g.suite.Require().NoError(err, "Failed to get current chain height")

	g.suite.T().Logf("🔍 Client state height: %d, Current chain height: %d", height, currentHeight)

	if height == 0 {
		g.suite.T().Fatalf("❌ CRITICAL ERROR: Invalid height 0 in client state")
	}

	if currentHeight > int64(height) {
		g.suite.T().Logf("📍 Using current chain height %d instead of client state height %d for proof generation", currentHeight, height)
		height = uint64(currentHeight)
	}

	if height > uint64(currentHeight)+100 {
		g.suite.T().Fatalf("❌ CRITICAL ERROR: Selected height %d is too far in future (current: %d)", height, currentHeight)
	}

	return height
}

func (g *SolanaFixtureGenerator) updateClientStateHeight(clientState map[string]interface{}, height uint64) map[string]interface{} {
	updatedClientState := make(map[string]interface{})
	for k, v := range clientState {
		updatedClientState[k] = v
	}
	updatedClientState["latest_height"] = height
	return updatedClientState
}

func (g *SolanaFixtureGenerator) queryProofForPath(ctx context.Context, chainA *cosmos.CosmosChain, keyPath string, height int64, expectExists bool) ([]byte, []byte) {
	value, proofBytes, err := g.queryPathWithProof(ctx, chainA, keyPath, height)

	if expectExists {
		g.suite.Require().NoError(err, "Failed to query membership path")
		g.suite.Require().NotEmpty(value, "Expected membership path to exist but got empty value")
		g.suite.Require().NotEmpty(proofBytes, "Got empty proof for membership path")
		g.suite.T().Logf("✅ Successfully queried membership path: value_len=%d, proof_len=%d", len(value), len(proofBytes))
	} else {
		if err != nil && !g.isKeyNotFoundError(err) {
			g.suite.T().Fatalf("❌ CRITICAL ERROR: Unexpected error querying non-membership path: %v", err)
		}
		g.suite.Require().Empty(value, "Expected non-membership path to not exist")
		if len(proofBytes) == 0 {
			g.suite.T().Logf("⚠️ Warning: Empty proof for non-membership path - setting empty proof")
		}
		value = []byte{}
		g.suite.T().Logf("✅ Successfully verified non-membership path: value is empty, proof_len=%d", len(proofBytes))
	}

	return value, proofBytes
}

func (g *SolanaFixtureGenerator) isKeyNotFoundError(err error) bool {
	return strings.Contains(err.Error(), "not found") || strings.Contains(err.Error(), "does not exist")
}

func (g *SolanaFixtureGenerator) createMembershipMessage(height uint64, proofBytes []byte, keyPath string, value []byte) map[string]interface{} {
	g.suite.Require().NotEmpty(keyPath, "Empty keyPath provided")

	return map[string]interface{}{
		"height":             height,
		"delay_time_period":  uint64(0),
		"delay_block_period": uint64(0),
		"proof":              hex.EncodeToString(proofBytes),
		"path":               hex.EncodeToString([]byte(keyPath)),
		"value":              hex.EncodeToString(value),
	}
}

func (g *SolanaFixtureGenerator) logMembershipFixtureDetails(keyPath string, expectExists bool, height uint64, value, proofBytes []byte) {
	fixtureType := "Membership"
	if !expectExists {
		fixtureType = "Non-membership"
	}

	g.suite.T().Logf("📦 Generated %s fixture:", fixtureType)
	g.suite.T().Logf("  - Height: %d", height)
	g.suite.T().Logf("  - Path: %s", keyPath)
	g.suite.T().Logf("  - Value length: %d bytes", len(value))
	g.suite.T().Logf("  - Proof length: %d bytes", len(proofBytes))
}

func (g *SolanaFixtureGenerator) createMembershipMetadata(keyPath string, expectExists bool, height uint64) map[string]interface{} {
	description := fmt.Sprintf("Membership verification fixture for path: %s", keyPath)
	if !expectExists {
		description = fmt.Sprintf("Non-membership verification fixture for path: %s", keyPath)
	}

	return map[string]interface{}{
		"generated_at": time.Now().UTC().Format(time.RFC3339),
		"source":       "real_cosmos_chain",
		"description":  description,
		"key_path":     keyPath,
		"height":       height,
	}
}

func (g *SolanaFixtureGenerator) queryPathWithProof(ctx context.Context, chainA *cosmos.CosmosChain, keyPath string, height int64) ([]byte, []byte, error) {
	g.suite.T().Logf("🔍 Querying real proof for path: %s at height: %d", keyPath, height)

	store, key, err := g.parseIBCPath(keyPath)
	if err != nil {
		return nil, nil, fmt.Errorf("failed to parse IBC path %s: %w", keyPath, err)
	}

	queryRes, err := g.performABCIQuery(ctx, chainA, store, key, height)
	if err != nil {
		return nil, nil, err
	}

	g.logQueryResult(queryRes, keyPath, height)

	proofBytes, err := g.processQueryProof(queryRes, keyPath, height)
	if err != nil {
		return nil, nil, err
	}

	if err := g.validateProofAgainstValue(proofBytes, queryRes.Value, keyPath); err != nil {
		return nil, nil, err
	}

	return queryRes.Value, proofBytes, nil
}

func (g *SolanaFixtureGenerator) performABCIQuery(ctx context.Context, chainA *cosmos.CosmosChain, store string, key []byte, height int64) (*abci.ResponseQuery, error) {
	g.suite.T().Logf("🔧 Querying store/%s/key with data: %x at proof height: %d", store, key, height)

	queryReq := &abci.RequestQuery{
		Path:   fmt.Sprintf("store/%s/key", store),
		Data:   key,
		Height: height,
		Prove:  true,
	}

	queryRes, err := e2esuite.ABCIQuery(ctx, chainA, queryReq)
	if err != nil {
		return nil, fmt.Errorf("ABCI query failed: %w", err)
	}

	if queryRes.Height != height {
		return nil, fmt.Errorf("proof height mismatch: expected %d, got %d", height, queryRes.Height)
	}

	return queryRes, nil
}

func (g *SolanaFixtureGenerator) logQueryResult(queryRes *abci.ResponseQuery, keyPath string, height int64) {
	g.suite.T().Logf("📊 Query result: height=%d, value_len=%d, has_proof=%t",
		queryRes.Height, len(queryRes.Value), queryRes.ProofOps != nil)

	if len(queryRes.Value) == 0 {
		g.suite.T().Logf("💭 Path %s does not exist at height %d - empty value returned", keyPath, height)
	} else {
		g.suite.T().Logf("✅ Path %s exists at height %d with %d bytes of data", keyPath, height, len(queryRes.Value))
	}
}

func (g *SolanaFixtureGenerator) processQueryProof(queryRes *abci.ResponseQuery, keyPath string, height int64) ([]byte, error) {
	if queryRes.ProofOps == nil || len(queryRes.ProofOps.Ops) == 0 {
		if len(queryRes.Value) > 0 {
			return nil, fmt.Errorf("expected proof for existing value at path %s but got no ProofOps", keyPath)
		}
		g.suite.T().Logf("💭 No proof returned from query (expected for non-membership)")
		return []byte{}, nil
	}

	g.suite.T().Logf("🔄 Converting Tendermint proof with %d operations", len(queryRes.ProofOps.Ops))

	merkleProof, err := g.convertTendermintProofOpsToICS(queryRes.ProofOps.Ops)
	if err != nil {
		return nil, fmt.Errorf("proof conversion failed: %w", err)
	}

	if len(merkleProof.Proofs) == 0 {
		return nil, fmt.Errorf("proof conversion resulted in zero commitment proofs for path %s", keyPath)
	}

	proofBytes, err := proto.Marshal(merkleProof)
	if err != nil {
		return nil, fmt.Errorf("proof marshal failed: %w", err)
	}

	if len(proofBytes) == 0 {
		return nil, fmt.Errorf("proof marshaling resulted in empty bytes for path %s", keyPath)
	}

	g.suite.T().Logf("✅ Generated proof with %d commitment proofs, total size: %d bytes",
		len(merkleProof.Proofs), len(proofBytes))

	return proofBytes, nil
}

func (g *SolanaFixtureGenerator) getCurrentClientState(ctx context.Context, chainA *cosmos.CosmosChain) map[string]interface{} {
	tmClientState := g.queryTendermintClientState(ctx, chainA)
	return g.convertClientStateToSolanaFormat(tmClientState, chainA.Config().ChainID)
}

func (g *SolanaFixtureGenerator) getCurrentConsensusState(ctx context.Context, chainA *cosmos.CosmosChain) map[string]interface{} {
	clientResp, err := e2esuite.GRPCQuery[clienttypes.QueryClientStateResponse](ctx, chainA, &clienttypes.QueryClientStateRequest{
		ClientId: ibctesting.FirstClientID,
	})
	g.suite.Require().NoError(err)
	g.suite.Require().NotNil(clientResp.ClientState)

	var tmClientState ibctmtypes.ClientState
	err = proto.Unmarshal(clientResp.ClientState.Value, &tmClientState)
	g.suite.Require().NoError(err)

	g.suite.T().Logf("🔍 Client state latest height: revision=%d, height=%d",
		tmClientState.LatestHeight.RevisionNumber, tmClientState.LatestHeight.RevisionHeight)

	resp, err := e2esuite.GRPCQuery[clienttypes.QueryConsensusStateResponse](ctx, chainA, &clienttypes.QueryConsensusStateRequest{
		ClientId:       ibctesting.FirstClientID,
		RevisionNumber: tmClientState.LatestHeight.RevisionNumber,
		RevisionHeight: tmClientState.LatestHeight.RevisionHeight,
		LatestHeight:   false,
	})
	g.suite.Require().NoError(err)
	g.suite.Require().NotNil(resp.ConsensusState)

	var tmConsensusState ibctmtypes.ConsensusState
	err = proto.Unmarshal(resp.ConsensusState.Value, &tmConsensusState)
	g.suite.Require().NoError(err)

	g.suite.T().Logf("✅ Retrieved consensus state for height: %d-%d",
		tmClientState.LatestHeight.RevisionNumber, tmClientState.LatestHeight.RevisionHeight)

	return g.convertConsensusStateToSolanaFormat(&tmConsensusState, chainA.Config().ChainID)
}

func (g *SolanaFixtureGenerator) parseIBCPath(keyPath string) (string, []byte, error) {
	if keyPath == "" {
		return "", nil, fmt.Errorf("empty keyPath provided")
	}

	store := "ibc"

	if !g.isValidIBCPath(keyPath) {
		return "", nil, fmt.Errorf("invalid IBC path format: %s", keyPath)
	}

	key := []byte(keyPath)

	if len(key) == 0 {
		return "", nil, fmt.Errorf("keyPath resulted in empty key bytes")
	}

	g.suite.T().Logf("📝 Parsed IBC path: store=%s, key=%s, key_len=%d", store, keyPath, len(key))
	return store, key, nil
}

func (g *SolanaFixtureGenerator) isValidIBCPath(path string) bool {
	validPrefixes := []string{
		"clients/", "connections/", "channelEnds/", "commitments/",
		"receipts/", "acks/", "nextSequenceSend/", "nextSequenceRecv/", "nextSequenceAck/",
	}

	for _, prefix := range validPrefixes {
		if strings.HasPrefix(path, prefix) {
			g.suite.T().Logf("✅ Path %s matches valid IBC prefix: %s", path, prefix)
			return true
		}
	}

	g.suite.T().Logf("❌ Path %s doesn't match any known IBC prefixes", path)
	return false
}

func (g *SolanaFixtureGenerator) convertTendermintProofOpsToICS(proofOps []tmcrypto.ProofOp) (*commitmenttypes.MerkleProof, error) {
	if len(proofOps) == 0 {
		return nil, fmt.Errorf("no proof ops provided")
	}

	g.suite.T().Logf("🔄 Converting Tendermint proof with %d operations", len(proofOps))

	var icsProofs []*ics23.CommitmentProof
	for i, op := range proofOps {
		g.suite.T().Logf("📋 Processing proof op %d: type=%s, data_len=%d", i, op.Type, len(op.Data))

		var commitmentProof ics23.CommitmentProof
		if err := proto.Unmarshal(op.Data, &commitmentProof); err != nil {
			return nil, fmt.Errorf("failed to unmarshal proof op %d: %w", i, err)
		}

		icsProofs = append(icsProofs, &commitmentProof)
	}

	if len(icsProofs) == 0 {
		return nil, fmt.Errorf("no valid proofs found in proof ops")
	}

	merkleProof := &commitmenttypes.MerkleProof{
		Proofs: icsProofs,
	}

	g.suite.T().Logf("✅ Converted to ICS commitment proof with %d proofs", len(merkleProof.Proofs))
	return merkleProof, nil
}

func (g *SolanaFixtureGenerator) validateProofAgainstValue(proof []byte, value []byte, path string) error {
	if len(proof) == 0 && len(value) > 0 {
		return fmt.Errorf("proof is empty but value exists for path %s", path)
	}

	proofType := "Non-membership"
	if len(value) > 0 {
		proofType = "Membership"
	}
	g.suite.T().Logf("🔍 %s case for path %s: value_len=%d, proof_len=%d", proofType, path, len(value), len(proof))

	if len(proof) > 0 {
		var merkleProof commitmenttypes.MerkleProof
		if err := proto.Unmarshal(proof, &merkleProof); err != nil {
			return fmt.Errorf("generated proof is not valid protobuf: %w", err)
		}

		if len(merkleProof.Proofs) == 0 {
			return fmt.Errorf("proof contains no commitment proofs")
		}

		g.suite.T().Logf("✅ Proof validation passed: %d commitment proofs", len(merkleProof.Proofs))
	}

	return nil
}

func (g *SolanaFixtureGenerator) getBlockchainConsensusStateAtHeight(ctx context.Context, chainA *cosmos.CosmosChain, height uint64) map[string]interface{} {
	g.suite.T().Logf("🔍 Getting BLOCKCHAIN consensus state at height %d", height)

	appHash, err := g.getAppHashAtHeight(ctx, chainA, int64(height))
	if err != nil {
		g.suite.T().Fatalf("❌ CRITICAL ERROR: Failed to get app hash at height %d: %v", height, err)
	}

	blockRes, err := chainA.Nodes()[0].Client.Block(ctx, ptr(int64(height)))
	if err != nil {
		g.suite.T().Fatalf("❌ CRITICAL ERROR: Failed to get block at height %d: %v", height, err)
	}

	g.suite.T().Logf("✅ Created blockchain consensus state at height %d with app hash: %s", height, appHash)

	return map[string]interface{}{
		"timestamp":            blockRes.Block.Header.Time.UnixNano(),
		"root":                 appHash,
		"next_validators_hash": hex.EncodeToString(blockRes.Block.Header.NextValidatorsHash),
	}
}

func (g *SolanaFixtureGenerator) getAppHashAtHeight(ctx context.Context, chainA *cosmos.CosmosChain, height int64) (string, error) {
	g.suite.T().Logf("🔍 Getting app hash at blockchain height %d", height)

	blockRes, err := chainA.Nodes()[0].Client.Block(ctx, &height)
	if err != nil {
		return "", fmt.Errorf("failed to get block at height %d: %w", height, err)
	}

	appHash := hex.EncodeToString(blockRes.Block.Header.AppHash)
	g.suite.T().Logf("✅ Got app hash at height %d: %s", height, appHash)

	return appHash, nil
}

func (g *SolanaFixtureGenerator) saveJsonFixture(filename string, data interface{}) {
	jsonData, err := json.MarshalIndent(data, "", "  ")
	g.suite.Require().NoError(err)

	err = os.WriteFile(filename, jsonData, 0644)
	g.suite.Require().NoError(err)
}

func (g *SolanaFixtureGenerator) createMetadata(description string) map[string]interface{} {
	return map[string]interface{}{
		"generated_at": time.Now().UTC().Format(time.RFC3339),
		"source":       "real_cosmos_chain",
		"description":  description,
	}
}

func ptr(i int64) *int64 {
	return &i
}
