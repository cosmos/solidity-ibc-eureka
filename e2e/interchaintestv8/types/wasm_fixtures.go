package types

import (
	"encoding/json"
	"fmt"
	"os"
	"strings"

	"github.com/stretchr/testify/suite"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
	ethereumtypes "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/ethereum"
)

type WasmFixtureGenerator struct {
	shouldGenerateFixture bool
	fixture               ethereumtypes.StepsFixture
}

// NewWasmFixtureGenerator creates a new WasmFixtureGenerator
// If shouldGenerateFixture is false, the generator will not generate any fixtures
func NewWasmFixtureGenerator(s *suite.Suite, shouldGenerateFixture bool) *WasmFixtureGenerator {
	wasmFixtureGenerator := &WasmFixtureGenerator{
		shouldGenerateFixture: shouldGenerateFixture,
	}

	fixtureName := getTopLevelTestName(s)
	if shouldGenerateFixture {
		s.T().Cleanup(func() {
			s.T().Logf("Writing fixtures for %s", fixtureName)
			if err := wasmFixtureGenerator.writeFixtures(fixtureName); err != nil {
				s.T().Logf("Error writing fixtures: %v", err)
			}
		})
	}

	return wasmFixtureGenerator
}

// AddFixtureStep adds a new fixture step that will be written to the fixture file at the end of the test
func (g *WasmFixtureGenerator) AddFixtureStep(stepName string, jsonMarshalble interface{}) {
	if !g.shouldGenerateFixture {
		return
	}

	g.fixture.Steps = append(g.fixture.Steps, ethereumtypes.Step{
		Name: stepName,
		Data: jsonMarshalble,
	})
}

func (g *WasmFixtureGenerator) ShouldGenerateFixture() bool {
	return g.shouldGenerateFixture
}

func (g *WasmFixtureGenerator) writeFixtures(fixtureName string) error {
	if !g.shouldGenerateFixture {
		return nil
	}

	filePath := fmt.Sprintf("%s/%s.json", testvalues.WasmFixturesDir, fixtureName)

	fmt.Printf("Writing %d fixtures to %s\n", len(g.fixture.Steps), filePath)
	fixturesBz, err := json.MarshalIndent(g.fixture, "", " ")
	if err != nil {
		return err
	}

	// nolint:gosec
	return os.WriteFile(filePath, fixturesBz, 0o644)
}

func getTopLevelTestName(s *suite.Suite) string {
	parts := strings.Split(s.T().Name(), "/")

	if len(parts) >= 2 {
		return parts[1]
	}

	return s.T().Name()
}
