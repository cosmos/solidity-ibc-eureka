package tendermint_light_client_fixtures

import (
	"context"
	"encoding/hex"
	"fmt"
	"path/filepath"

	"github.com/cosmos/gogoproto/proto"
	ics23 "github.com/cosmos/ics23/go"

	cmtservice "github.com/cosmos/cosmos-sdk/client/grpc/cmtservice"

	abci "github.com/cometbft/cometbft/abci/types"
	cmtcrypto "github.com/cometbft/cometbft/proto/tendermint/crypto"

	clienttypes "github.com/cosmos/ibc-go/v10/modules/core/02-client/types"
	commitmenttypes "github.com/cosmos/ibc-go/v10/modules/core/23-commitment/types"
	ibcexported "github.com/cosmos/ibc-go/v10/modules/core/exported"
	ibctmtypes "github.com/cosmos/ibc-go/v10/modules/light-clients/07-tendermint"

	"github.com/strangelove-ventures/interchaintest/v8/chain/cosmos"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/e2esuite"
)

type KeyPath struct {
	Key        string
	Membership bool
}

type ProofContext struct {
	KeyPath          string
	ExpectMembership bool
	ProofHeight      uint64
	BlockHeight      uint64
	ActualAppHash    []byte
	ABCIResponse     *abci.ResponseQuery
	ConsensusState   *ibctmtypes.ConsensusState
}

type MembershipFixtureGenerator struct {
	generator FixtureGeneratorInterface
}

func NewMembershipFixtureGenerator(generator FixtureGeneratorInterface) *MembershipFixtureGenerator {
	return &MembershipFixtureGenerator{
		generator: generator,
	}
}

func (g *MembershipFixtureGenerator) GenerateMembershipVerificationScenariosWithPredefinedKeys(
	ctx context.Context,
	chain *cosmos.CosmosChain,
	keyPaths []KeyPath,
	clientId string,
) {
	if !g.generator.IsEnabled() {
		return
	}

	g.generator.LogInfo("🔧 Generating membership verification scenarios with predefined keys")

	for i, keySpec := range keyPaths {
		proofType := g.proofTypeNameFor(keySpec.Membership)
		g.generator.LogInfof("🔍 Processing predefined key path: %s (%s)", keySpec.Key, proofType)
		g.generateFixtureForKeyPath(ctx, chain, keySpec.Key, i, keySpec.Membership, clientId)
	}

	g.generator.LogInfo("✅ Predefined key membership scenarios generated successfully")
}

func (g *MembershipFixtureGenerator) generateFixtureForKeyPath(
	ctx context.Context,
	chain *cosmos.CosmosChain,
	keyPath string,
	index int,
	expectMembership bool,
	clientId string,
) {
	proofType := g.proofTypeNameFor(expectMembership)
	g.generator.LogInfof("🔧 Generating %s fixture for key: %s", proofType, keyPath)

	proofHeight, latestConsensusState := g.getLatestConsensusStateHeight(ctx, chain, clientId)
	g.generator.LogInfof("📊 Using latest consensus state at height %d for proof generation", proofHeight)

	blockHeight, actualAppHash := g.getAppHashFromBlock(ctx, chain, proofHeight)
	abciResp := g.queryStateProofForKey(ctx, chain, keyPath, proofHeight)
	g.ensureProofMatchesExpectation(abciResp, keyPath, expectMembership)
	tmConsensusState := g.unmarshalConsensusState(latestConsensusState, actualAppHash)
	merkleProofBytes := g.convertToIBCMerkleProof(abciResp.ProofOps)

	g.ensureHeightMatches(abciResp.Height, proofHeight)

	proofCtx := &ProofContext{
		KeyPath:          keyPath,
		ExpectMembership: expectMembership,
		ProofHeight:      proofHeight,
		BlockHeight:      blockHeight,
		ActualAppHash:    actualAppHash,
		ABCIResponse:     abciResp,
		ConsensusState:   tmConsensusState,
	}

	g.saveFixture(ctx, chain, proofCtx, merkleProofBytes, index, clientId)
}

func (g *MembershipFixtureGenerator) proofTypeNameFor(isMembership bool) string {
	if isMembership {
		return "membership"
	}
	return "non-membership"
}

func (g *MembershipFixtureGenerator) getLatestConsensusStateHeight(
	ctx context.Context,
	chain *cosmos.CosmosChain,
	clientId string,
) (uint64, *clienttypes.ConsensusStateWithHeight) {
	allStatesResp, err := e2esuite.GRPCQuery[clienttypes.QueryConsensusStatesResponse](
		ctx,
		chain,
		&clienttypes.QueryConsensusStatesRequest{
			ClientId: clientId,
		},
	)
	g.generator.RequireNoError(err)
	g.generator.RequireNotNil(allStatesResp.ConsensusStates, "No consensus states found for client")
	g.generator.RequireGreater(len(allStatesResp.ConsensusStates), 0, "No consensus states found for client")

	latest := g.findHighestRevisionHeight(allStatesResp.ConsensusStates)
	return latest.Height.RevisionHeight, latest
}

func (g *MembershipFixtureGenerator) findHighestRevisionHeight(
	states []clienttypes.ConsensusStateWithHeight,
) *clienttypes.ConsensusStateWithHeight {
	var highest *clienttypes.ConsensusStateWithHeight
	for _, state := range states {
		if highest == nil || state.Height.RevisionHeight > highest.Height.RevisionHeight {
			stateCopy := state
			highest = &stateCopy
		}
	}
	return highest
}

func (g *MembershipFixtureGenerator) getAppHashFromBlock(
	ctx context.Context,
	chain *cosmos.CosmosChain,
	proofHeight uint64,
) (uint64, []byte) {
	blockHeight := proofHeight + 1
	g.generator.LogInfof("🔍 Fetching block at height %d to get app hash for state at height %d", blockHeight, proofHeight)

	blockResp, err := e2esuite.GRPCQuery[cmtservice.GetBlockByHeightResponse](
		ctx,
		chain,
		&cmtservice.GetBlockByHeightRequest{
			Height: int64(blockHeight),
		},
	)
	g.generator.RequireNoError(err)

	appHash := blockResp.Block.Header.AppHash
	g.generator.LogInfof("📦 Block %d app hash: %s", blockHeight, hex.EncodeToString(appHash))

	return blockHeight, appHash
}

func (g *MembershipFixtureGenerator) queryStateProofForKey(
	ctx context.Context,
	chain *cosmos.CosmosChain,
	keyPath string,
	proofHeight uint64,
) *abci.ResponseQuery {
	storePath := fmt.Sprintf("store/%s/key", ibcexported.StoreKey)

	abciReq := &abci.RequestQuery{
		Path:   storePath,
		Data:   []byte(keyPath),
		Height: int64(proofHeight),
		Prove:  true,
	}

	g.generator.LogInfof("📡 ABCI Query: path=%s, data=%s, height=%d, prove=true",
		storePath, keyPath, abciReq.Height)

	abciResp, err := e2esuite.ABCIQuery(ctx, chain, abciReq)
	g.generator.RequireNoError(err)

	return abciResp
}

func (g *MembershipFixtureGenerator) ensureProofMatchesExpectation(
	abciResp *abci.ResponseQuery,
	keyPath string,
	expectMembership bool,
) {
	hasValue := len(abciResp.Value) > 0

	if expectMembership && !hasValue {
		g.generator.Fatalf("❌ Expected membership proof for key %s but value is empty", keyPath)
	}
	if !expectMembership && hasValue {
		g.generator.Fatalf("❌ Expected non-membership proof for key %s but value exists (length: %d)",
			keyPath, len(abciResp.Value))
	}

	proofType := g.proofTypeNameFor(expectMembership)
	if hasValue {
		g.generator.LogInfof("✅ ABCI query successful - %s case: value length: %d, proof ops: %d",
			proofType, len(abciResp.Value), len(abciResp.ProofOps.Ops))
	} else {
		g.generator.LogInfof("✅ ABCI query successful - %s case: empty value, proof ops: %d",
			proofType, len(abciResp.ProofOps.Ops))
	}

	if len(abciResp.ProofOps.Ops) == 0 {
		g.generator.Fatalf("❌ ABCI proof is empty for key: %s", keyPath)
	}
}

func (g *MembershipFixtureGenerator) unmarshalConsensusState(
	consensusStateWithHeight *clienttypes.ConsensusStateWithHeight,
	actualAppHash []byte,
) *ibctmtypes.ConsensusState {
	var tmConsensusState ibctmtypes.ConsensusState
	err := proto.Unmarshal(consensusStateWithHeight.ConsensusState.Value, &tmConsensusState)
	g.generator.RequireNoError(err)

	g.generator.LogInfof("📊 Original consensus state root: %s", hex.EncodeToString(tmConsensusState.Root.GetHash()))
	g.generator.LogInfof("📊 Actual app hash from block: %s", hex.EncodeToString(actualAppHash))

	return &tmConsensusState
}

func (g *MembershipFixtureGenerator) convertToIBCMerkleProof(proofOps *cmtcrypto.ProofOps) []byte {
	commitmentProofs := g.extractCommitmentProofs(proofOps)
	merkleProof := &commitmenttypes.MerkleProof{Proofs: commitmentProofs}

	proofBytes, err := proto.Marshal(merkleProof)
	g.generator.RequireNoError(err)

	g.generator.LogInfof("🔄 Converted %d ABCI ProofOps to IBC MerkleProof (%d bytes)",
		len(proofOps.Ops), len(proofBytes))

	return proofBytes
}

func (g *MembershipFixtureGenerator) extractCommitmentProofs(proofOps *cmtcrypto.ProofOps) []*ics23.CommitmentProof {
	commitmentProofs := make([]*ics23.CommitmentProof, len(proofOps.Ops))

	for i, op := range proofOps.Ops {
		var commitmentProof ics23.CommitmentProof
		err := proto.Unmarshal(op.Data, &commitmentProof)
		if err != nil {
			g.generator.Fatalf("Failed to unmarshal CommitmentProof from ProofOp[%d]: %v", i, err)
		}
		commitmentProofs[i] = &commitmentProof
	}

	return commitmentProofs
}

func (g *MembershipFixtureGenerator) ensureHeightMatches(actualHeight int64, expectedHeight uint64) {
	if uint64(actualHeight) != expectedHeight {
		g.generator.Fatalf("❌ ABCI returned unexpected height: got %d, expected %d",
			actualHeight, expectedHeight)
	}
}

func (g *MembershipFixtureGenerator) saveFixture(
	ctx context.Context,
	chain *cosmos.CosmosChain,
	proofCtx *ProofContext,
	proofBytes []byte,
	index int,
	clientId string,
) {
	proofType := g.proofTypeNameFor(proofCtx.ExpectMembership)

	membershipMsg := g.buildMembershipMessage(proofCtx, proofBytes, proofType)
	clientState := g.buildClientState(ctx, chain, clientId)
	consensusState := g.buildConsensusState(proofCtx)

	scenarioName := fmt.Sprintf("%s_key_%d", proofType, index)
	fixture := g.assembleFixture(
		scenarioName,
		clientState,
		consensusState,
		membershipMsg,
		proofCtx,
		proofType,
		chain.Config().ChainID,
	)

	filename := filepath.Join(g.generator.GetFixtureDir(),
		fmt.Sprintf("verify_%s_key_%d.json", proofType, index))
	g.generator.SaveJsonFixture(filename, fixture)
	g.generator.LogInfof("💾 %s fixture saved: %s", proofType, filename)
}

func (g *MembershipFixtureGenerator) buildMembershipMessage(
	proofCtx *ProofContext,
	proofBytes []byte,
	proofType string,
) map[string]interface{} {
	metadata := g.generator.CreateMetadata(
		fmt.Sprintf("Valid %s proof for key: %s", proofType, proofCtx.KeyPath))
	metadata["proof_format"] = "hex-encoded protobuf"
	metadata["proof_type_details"] = "ibc.core.commitment.v1.MerkleProof"
	metadata["proof_conversion"] = "Converted from ABCI ProofOps to IBC MerkleProof format"
	metadata["proof_size_bytes"] = len(proofBytes)

	return map[string]interface{}{
		"height":             proofCtx.ProofHeight,
		"delay_time_period":  0,
		"delay_block_period": 0,
		"proof":              hex.EncodeToString(proofBytes),
		"path":               []string{string(ibcexported.StoreKey), proofCtx.KeyPath},
		"value":              hex.EncodeToString(proofCtx.ABCIResponse.Value),
		"metadata":           metadata,
	}
}

func (g *MembershipFixtureGenerator) buildClientState(
	ctx context.Context,
	chain *cosmos.CosmosChain,
	clientId string,
) map[string]interface{} {
	tmClientState := g.generator.QueryTendermintClientState(ctx, chain, clientId)
	return g.generator.ConvertClientStateToFixtureFormat(tmClientState, chain.Config().ChainID)
}

func (g *MembershipFixtureGenerator) buildConsensusState(proofCtx *ProofContext) map[string]interface{} {
	return map[string]interface{}{
		"timestamp":            proofCtx.ConsensusState.Timestamp.UnixNano(),
		"root":                 hex.EncodeToString(proofCtx.ActualAppHash),
		"next_validators_hash": hex.EncodeToString(proofCtx.ConsensusState.NextValidatorsHash),
		"metadata": g.generator.CreateMetadata(
			fmt.Sprintf("Consensus state at height %d (app hash from block %d)",
				proofCtx.ProofHeight, proofCtx.BlockHeight)),
	}
}

func (g *MembershipFixtureGenerator) assembleFixture(
	scenarioName string,
	clientState map[string]interface{},
	consensusState map[string]interface{},
	membershipMsg map[string]interface{},
	proofCtx *ProofContext,
	proofType string,
	chainID string,
) map[string]interface{} {
	return map[string]interface{}{
		"scenario":        scenarioName,
		"client_state":    clientState,
		"consensus_state": consensusState,
		"membership_msg":  membershipMsg,
		"key_info": map[string]interface{}{
			"path":        proofCtx.KeyPath,
			"value_size":  len(proofCtx.ABCIResponse.Value),
			"description": fmt.Sprintf("IBC key: %s (%s)", proofCtx.KeyPath, proofType),
			"proof_type":  proofType,
		},
		"metadata": g.generator.CreateUnifiedMetadata(scenarioName, chainID),
	}
}
