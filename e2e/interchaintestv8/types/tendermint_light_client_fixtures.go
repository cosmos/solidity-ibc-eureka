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

type TendermintLightClientFixtureGenerator struct {
	Enabled    bool
	FixtureDir string
	suite      *suite.Suite

	updateClientGenerator *tendermint_light_client_fixtures.UpdateClientFixtureGenerator
	membershipGenerator   *tendermint_light_client_fixtures.MembershipFixtureGenerator
	utils                 *tendermint_light_client_fixtures.FixtureGeneratorUtils
}

func NewTendermintLightClientFixtureGenerator(s *suite.Suite) *TendermintLightClientFixtureGenerator {
	generator := &TendermintLightClientFixtureGenerator{
		Enabled: isFixtureGenerationEnabled(),
		suite:   s,
	}

	if generator.Enabled {
		generator.initializeFixtureDirectory(s)
		generator.initializeSubmodules(s)
	}

	return generator
}

func isFixtureGenerationEnabled() bool {
	return os.Getenv(testvalues.EnvKeyGenerateTendermintLightClientFixtures) == testvalues.EnvValueGenerateFixtures_True
}

func (g *TendermintLightClientFixtureGenerator) initializeFixtureDirectory(s *suite.Suite) {
	absPath, err := filepath.Abs(filepath.Join("../..", testvalues.TendermintLightClientFixturesDir))
	if err != nil {
		s.T().Fatalf("Failed to get absolute path for fixtures: %v", err)
	}

	g.FixtureDir = absPath

	if err := os.MkdirAll(g.FixtureDir, 0o755); err != nil {
		s.T().Fatalf("Failed to create Tendermint light client fixture directory: %v", err)
	}

	s.T().Logf("📁 Tendermint light client fixtures will be saved to: %s", g.FixtureDir)
}

func (g *TendermintLightClientFixtureGenerator) initializeSubmodules(s *suite.Suite) {
	g.utils = tendermint_light_client_fixtures.NewFixtureGeneratorUtils(s, g.FixtureDir)
	g.updateClientGenerator = tendermint_light_client_fixtures.NewUpdateClientFixtureGenerator(g)
	g.membershipGenerator = tendermint_light_client_fixtures.NewMembershipFixtureGenerator(g)
}

// Core Interface Implementation

func (g *TendermintLightClientFixtureGenerator) IsEnabled() bool {
	return g.Enabled
}

func (g *TendermintLightClientFixtureGenerator) GetFixtureDir() string {
	return g.FixtureDir
}

// Public API - Main Fixture Generation Methods

func (g *TendermintLightClientFixtureGenerator) GenerateMultipleUpdateClientScenarios(
	ctx context.Context,
	chainA *cosmos.CosmosChain,
	updateTxBodyBz []byte,
) {
	g.updateClientGenerator.GenerateMultipleUpdateClientScenarios(ctx, chainA, updateTxBodyBz)
}

func (g *TendermintLightClientFixtureGenerator) GenerateMembershipVerificationScenariosWithPredefinedKeys(
	ctx context.Context,
	chainA *cosmos.CosmosChain,
	keyPaths []KeyPath,
) {
	g.membershipGenerator.GenerateMembershipVerificationScenariosWithPredefinedKeys(ctx, chainA, keyPaths)
}

// Delegated Logging Operations

func (g *TendermintLightClientFixtureGenerator) LogInfo(msg string) {
	g.utils.LogInfo(msg)
}

func (g *TendermintLightClientFixtureGenerator) LogInfof(format string, args ...interface{}) {
	g.utils.LogInfof(format, args...)
}

func (g *TendermintLightClientFixtureGenerator) Fatalf(format string, args ...interface{}) {
	g.utils.Fatalf(format, args...)
}

// Delegated Test Assertions

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

// Delegated File Operations

func (g *TendermintLightClientFixtureGenerator) SaveJsonFixture(filename string, data interface{}) {
	g.utils.SaveJsonFixture(filename, data)
}

// Delegated Blockchain Queries

func (g *TendermintLightClientFixtureGenerator) QueryTendermintClientState(
	ctx context.Context,
	chainA *cosmos.CosmosChain,
) *ibctmtypes.ClientState {
	return g.utils.QueryTendermintClientState(ctx, chainA)
}

func (g *TendermintLightClientFixtureGenerator) QueryTendermintConsensusState(
	ctx context.Context,
	chainA *cosmos.CosmosChain,
) *ibctmtypes.ConsensusState {
	return g.utils.QueryTendermintConsensusState(ctx, chainA)
}

// Delegated Data Conversions

func (g *TendermintLightClientFixtureGenerator) ConvertClientStateToFixtureFormat(
	tmClientState *ibctmtypes.ClientState,
	chainID string,
) map[string]interface{} {
	return g.utils.ConvertClientStateToFixtureFormat(tmClientState, chainID)
}

func (g *TendermintLightClientFixtureGenerator) ConvertConsensusStateToFixtureFormat(
	tmConsensusState *ibctmtypes.ConsensusState,
	chainID string,
) map[string]interface{} {
	return g.utils.ConvertConsensusStateToFixtureFormat(tmConsensusState, chainID)
}

// Delegated Metadata Operations

func (g *TendermintLightClientFixtureGenerator) CreateMetadata(description string) map[string]interface{} {
	return g.utils.CreateMetadata(description)
}

func (g *TendermintLightClientFixtureGenerator) CreateUnifiedMetadata(
	scenarioName string,
	chainID string,
) map[string]interface{} {
	return g.utils.CreateUnifiedMetadata(scenarioName, chainID)
}
