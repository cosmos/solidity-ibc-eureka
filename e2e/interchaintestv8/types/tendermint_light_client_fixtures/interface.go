package tendermint_light_client_fixtures

import (
	"context"

	ibctmtypes "github.com/cosmos/ibc-go/v10/modules/light-clients/07-tendermint"

	"github.com/strangelove-ventures/interchaintest/v8/chain/cosmos"
)

// FixtureGeneratorInterface defines the common interface for accessing the main fixture generator
type FixtureGeneratorInterface interface {
	// Configuration methods
	IsEnabled() bool
	GetFixtureDir() string

	// Logging methods
	LogInfo(msg string)
	LogInfof(format string, args ...interface{})
	Fatalf(format string, args ...interface{})

	// Assertion methods
	RequireNoError(err error, msgAndArgs ...interface{})
	RequireLen(object interface{}, length int, msgAndArgs ...interface{})
	RequireNotNil(object interface{}, msgAndArgs ...interface{})
	RequireTrue(value bool, msgAndArgs ...interface{})
	RequireFileExists(path string, msgAndArgs ...interface{})
	RequireGreater(e1, e2 interface{}, msgAndArgs ...interface{})

	// File operations
	SaveJsonFixture(filename string, data interface{})

	// Query methods
	QueryTendermintClientState(ctx context.Context, chainA *cosmos.CosmosChain) *ibctmtypes.ClientState
	QueryTendermintConsensusState(ctx context.Context, chainA *cosmos.CosmosChain) *ibctmtypes.ConsensusState

	// Conversion methods
	ConvertClientStateToFixtureFormat(tmClientState *ibctmtypes.ClientState, chainID string) map[string]interface{}
	ConvertConsensusStateToFixtureFormat(tmConsensusState *ibctmtypes.ConsensusState, chainID string) map[string]interface{}

	// Metadata creation methods
	CreateMetadata(description string) map[string]interface{}
	CreateUnifiedMetadata(scenarioName, chainID string) map[string]interface{}
}
