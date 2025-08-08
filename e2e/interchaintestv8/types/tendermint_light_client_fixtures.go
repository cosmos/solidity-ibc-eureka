package types

import (
	"context"
	"os"
	"path/filepath"

	"github.com/stretchr/testify/suite"

	ibctmtypes "github.com/cosmos/ibc-go/v10/modules/light-clients/07-tendermint"

	"github.com/strangelove-ventures/interchaintest/v8/chain/cosmos"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/tendermint_light_client_fixtures"
)

type KeyPath = tendermint_light_client_fixtures.KeyPath

// TendermintLightClientFixtureGenerator manages fixture generation for tendermint light client tests
type TendermintLightClientFixtureGenerator struct {
	Enabled    bool
	FixtureDir string
	suite      *suite.Suite

	// Submodule generators
	updateClientGenerator *tendermint_light_client_fixtures.UpdateClientFixtureGenerator
	membershipGenerator   *tendermint_light_client_fixtures.MembershipFixtureGenerator
	utils                 *tendermint_light_client_fixtures.FixtureGeneratorUtils
}

// NewTendermintLightClientFixtureGenerator creates a new fixture generator with submodules
func NewTendermintLightClientFixtureGenerator(s *suite.Suite) *TendermintLightClientFixtureGenerator {
	generator := &TendermintLightClientFixtureGenerator{
		Enabled: os.Getenv(testvalues.EnvKeyGenerateTendermintLightClientFixtures) == testvalues.EnvValueGenerateFixtures_True,
		suite:   s,
	}

	if generator.Enabled {
		absPath, err := filepath.Abs(filepath.Join("../..", testvalues.TendermintLightClientFixturesDir))
		if err != nil {
			s.T().Fatalf("Failed to get absolute path for fixtures: %v", err)
		}
		generator.FixtureDir = absPath

		if err := os.MkdirAll(generator.FixtureDir, 0o755); err != nil {
			s.T().Fatalf("Failed to create Tendermint light client fixture directory: %v", err)
		}
		s.T().Logf("📁 Tendermint light client fixtures will be saved to: %s", generator.FixtureDir)

		// Initialize utility functions
		generator.utils = tendermint_light_client_fixtures.NewFixtureGeneratorUtils(s, generator.FixtureDir)

		// Initialize submodule generators
		generator.updateClientGenerator = tendermint_light_client_fixtures.NewUpdateClientFixtureGenerator(generator)
		generator.membershipGenerator = tendermint_light_client_fixtures.NewMembershipFixtureGenerator(generator)
	}

	return generator
}

// Implement FixtureGeneratorInterface

func (g *TendermintLightClientFixtureGenerator) IsEnabled() bool {
	return g.Enabled
}

func (g *TendermintLightClientFixtureGenerator) GetFixtureDir() string {
	return g.FixtureDir
}

// Logging methods
func (g *TendermintLightClientFixtureGenerator) LogInfo(msg string) {
	g.utils.LogInfo(msg)
}

func (g *TendermintLightClientFixtureGenerator) LogInfof(format string, args ...interface{}) {
	g.utils.LogInfof(format, args...)
}

func (g *TendermintLightClientFixtureGenerator) Fatalf(format string, args ...interface{}) {
	g.utils.Fatalf(format, args...)
}

// Assertion methods
func (g *TendermintLightClientFixtureGenerator) RequireNoError(err error, msgAndArgs ...interface{}) {
	g.utils.RequireNoError(err, msgAndArgs...)
}

func (g *TendermintLightClientFixtureGenerator) RequireLen(object interface{}, length int, msgAndArgs ...interface{}) {
	g.utils.RequireLen(object, length, msgAndArgs...)
}

func (g *TendermintLightClientFixtureGenerator) RequireNotNil(object interface{}, msgAndArgs ...interface{}) {
	g.utils.RequireNotNil(object, msgAndArgs...)
}

func (g *TendermintLightClientFixtureGenerator) RequireTrue(value bool, msgAndArgs ...interface{}) {
	g.utils.RequireTrue(value, msgAndArgs...)
}

func (g *TendermintLightClientFixtureGenerator) RequireFileExists(path string, msgAndArgs ...interface{}) {
	g.utils.RequireFileExists(path, msgAndArgs...)
}

func (g *TendermintLightClientFixtureGenerator) RequireGreater(e1, e2 interface{}, msgAndArgs ...interface{}) {
	g.utils.RequireGreater(e1, e2, msgAndArgs...)
}

// File operations
func (g *TendermintLightClientFixtureGenerator) SaveJsonFixture(filename string, data interface{}) {
	g.utils.SaveJsonFixture(filename, data)
}

// Query methods
func (g *TendermintLightClientFixtureGenerator) QueryTendermintClientState(ctx context.Context, chainA *cosmos.CosmosChain) *ibctmtypes.ClientState {
	return g.utils.QueryTendermintClientState(ctx, chainA)
}

func (g *TendermintLightClientFixtureGenerator) QueryTendermintConsensusState(ctx context.Context, chainA *cosmos.CosmosChain) *ibctmtypes.ConsensusState {
	return g.utils.QueryTendermintConsensusState(ctx, chainA)
}

// Conversion methods
func (g *TendermintLightClientFixtureGenerator) ConvertClientStateToFixtureFormat(tmClientState *ibctmtypes.ClientState, chainID string) map[string]interface{} {
	return g.utils.ConvertClientStateToFixtureFormat(tmClientState, chainID)
}

func (g *TendermintLightClientFixtureGenerator) ConvertConsensusStateToFixtureFormat(tmConsensusState *ibctmtypes.ConsensusState, chainID string) map[string]interface{} {
	return g.utils.ConvertConsensusStateToFixtureFormat(tmConsensusState, chainID)
}

// Metadata creation methods
func (g *TendermintLightClientFixtureGenerator) CreateMetadata(description string) map[string]interface{} {
	return g.utils.CreateMetadata(description)
}

func (g *TendermintLightClientFixtureGenerator) CreateUnifiedMetadata(scenarioName, chainID string) map[string]interface{} {
	return g.utils.CreateUnifiedMetadata(scenarioName, chainID)
}

// Public API methods that delegate to submodules

// GenerateMultipleUpdateClientScenarios delegates to the update client submodule
func (g *TendermintLightClientFixtureGenerator) GenerateMultipleUpdateClientScenarios(ctx context.Context, chainA *cosmos.CosmosChain, updateTxBodyBz []byte) {
	g.updateClientGenerator.GenerateMultipleUpdateClientScenarios(ctx, chainA, updateTxBodyBz)
}

// GenerateMembershipVerificationScenariosWithPredefinedKeys delegates to the membership submodule
func (g *TendermintLightClientFixtureGenerator) GenerateMembershipVerificationScenariosWithPredefinedKeys(ctx context.Context, chainA *cosmos.CosmosChain, keyPaths []KeyPath) {
	g.membershipGenerator.GenerateMembershipVerificationScenariosWithPredefinedKeys(ctx, chainA, keyPaths)
}
