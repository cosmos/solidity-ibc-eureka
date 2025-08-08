package tendermint_light_client_fixtures

import (
	"context"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"time"

	"github.com/cosmos/gogoproto/proto"
	"github.com/stretchr/testify/suite"

	clienttypes "github.com/cosmos/ibc-go/v10/modules/core/02-client/types"
	ibctmtypes "github.com/cosmos/ibc-go/v10/modules/light-clients/07-tendermint"
	ibctesting "github.com/cosmos/ibc-go/v10/testing"

	"github.com/strangelove-ventures/interchaintest/v8/chain/cosmos"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/e2esuite"
)

// FixtureGeneratorUtils contains common utility functions for fixture generation
type FixtureGeneratorUtils struct {
	suite      *suite.Suite
	fixtureDir string
}

// NewFixtureGeneratorUtils creates a new utility instance
func NewFixtureGeneratorUtils(s *suite.Suite, fixtureDir string) *FixtureGeneratorUtils {
	return &FixtureGeneratorUtils{
		suite:      s,
		fixtureDir: fixtureDir,
	}
}

// Logging methods

func (u *FixtureGeneratorUtils) LogInfo(msg string) {
	u.suite.T().Log(msg)
}

func (u *FixtureGeneratorUtils) LogInfof(format string, args ...interface{}) {
	u.suite.T().Logf(format, args...)
}

func (u *FixtureGeneratorUtils) Fatalf(format string, args ...interface{}) {
	u.suite.T().Fatalf(format, args...)
}

// Assertion methods

func (u *FixtureGeneratorUtils) RequireNoError(err error, msgAndArgs ...interface{}) {
	u.suite.Require().NoError(err, msgAndArgs...)
}

func (u *FixtureGeneratorUtils) RequireLen(object interface{}, length int, msgAndArgs ...interface{}) {
	u.suite.Require().Len(object, length, msgAndArgs...)
}

func (u *FixtureGeneratorUtils) RequireNotNil(object interface{}, msgAndArgs ...interface{}) {
	u.suite.Require().NotNil(object, msgAndArgs...)
}

func (u *FixtureGeneratorUtils) RequireTrue(value bool, msgAndArgs ...interface{}) {
	u.suite.Require().True(value, msgAndArgs...)
}

func (u *FixtureGeneratorUtils) RequireFileExists(path string, msgAndArgs ...interface{}) {
	u.suite.Require().FileExists(path, msgAndArgs...)
}

func (u *FixtureGeneratorUtils) RequireGreater(e1, e2 interface{}, msgAndArgs ...interface{}) {
	u.suite.Require().Greater(e1, e2, msgAndArgs...)
}

// Configuration methods

func (u *FixtureGeneratorUtils) GetFixtureDir() string {
	return u.fixtureDir
}

// File operations

func (u *FixtureGeneratorUtils) SaveJsonFixture(filename string, data interface{}) {
	jsonData, err := json.MarshalIndent(data, "", "  ")
	u.suite.Require().NoError(err)

	err = os.WriteFile(filename, jsonData, 0o600)
	u.suite.Require().NoError(err)
}

// Query methods

func (u *FixtureGeneratorUtils) QueryTendermintClientState(ctx context.Context, chainA *cosmos.CosmosChain) *ibctmtypes.ClientState {
	resp, err := e2esuite.GRPCQuery[clienttypes.QueryClientStateResponse](ctx, chainA, &clienttypes.QueryClientStateRequest{
		ClientId: ibctesting.FirstClientID,
	})
	u.suite.Require().NoError(err)
	u.suite.Require().NotNil(resp.ClientState)

	var tmClientState ibctmtypes.ClientState
	err = proto.Unmarshal(resp.ClientState.Value, &tmClientState)
	u.suite.Require().NoError(err)

	return &tmClientState
}

func (u *FixtureGeneratorUtils) QueryTendermintConsensusState(ctx context.Context, chainA *cosmos.CosmosChain) *ibctmtypes.ConsensusState {
	resp, err := e2esuite.GRPCQuery[clienttypes.QueryConsensusStateResponse](ctx, chainA, &clienttypes.QueryConsensusStateRequest{
		ClientId:       ibctesting.FirstClientID,
		RevisionNumber: 1,
		RevisionHeight: 1,
		LatestHeight:   true,
	})
	u.suite.Require().NoError(err)
	u.suite.Require().NotNil(resp.ConsensusState)

	var tmConsensusState ibctmtypes.ConsensusState
	err = proto.Unmarshal(resp.ConsensusState.Value, &tmConsensusState)
	u.suite.Require().NoError(err)

	return &tmConsensusState
}

// Conversion methods

func (u *FixtureGeneratorUtils) ConvertClientStateToFixtureFormat(tmClientState *ibctmtypes.ClientState, chainID string) map[string]interface{} {
	return map[string]interface{}{
		"chain_id":                tmClientState.ChainId,
		"trust_level_numerator":   tmClientState.TrustLevel.Numerator,
		"trust_level_denominator": tmClientState.TrustLevel.Denominator,
		"trusting_period":         tmClientState.TrustingPeriod.Seconds(),
		"unbonding_period":        tmClientState.UnbondingPeriod.Seconds(),
		"max_clock_drift":         tmClientState.MaxClockDrift.Seconds(),
		"frozen_height":           tmClientState.FrozenHeight.RevisionHeight,
		"latest_height":           tmClientState.LatestHeight.RevisionHeight,
		"metadata":                u.CreateMetadata(fmt.Sprintf("Client state for %s captured from %s", tmClientState.ChainId, chainID)),
	}
}

func (u *FixtureGeneratorUtils) ConvertConsensusStateToFixtureFormat(tmConsensusState *ibctmtypes.ConsensusState, chainID string) map[string]interface{} {
	return map[string]interface{}{
		"timestamp":            tmConsensusState.Timestamp.UnixNano(),
		"root":                 hex.EncodeToString(tmConsensusState.Root.GetHash()),
		"next_validators_hash": hex.EncodeToString(tmConsensusState.NextValidatorsHash),
		"metadata":             u.CreateMetadata(fmt.Sprintf("Consensus state captured from %s", chainID)),
	}
}

// Metadata creation methods

func (u *FixtureGeneratorUtils) CreateMetadata(description string) map[string]interface{} {
	return map[string]interface{}{
		"generated_at": time.Now().UTC().Format(time.RFC3339),
		"source":       "real_cosmos_chain",
		"description":  description,
	}
}

func (u *FixtureGeneratorUtils) CreateUnifiedMetadata(scenarioName, chainID string) map[string]interface{} {
	return map[string]interface{}{
		"generated_at": time.Now().UTC().Format(time.RFC3339),
		"source":       "real_cosmos_chain",
		"description":  fmt.Sprintf("Unified tendermint light client fixture for scenario: %s", scenarioName),
		"scenario":     scenarioName,
		"chain_id":     chainID,
	}
}
