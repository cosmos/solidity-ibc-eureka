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

type RustFixtureGenerator struct {
	shouldGenerateFixture bool
	fixture               ethereumtypes.StepsFixture
}

// NewRustFixtureGenerator creates a new RustFixtureGenerator

// If shouldGenerateFixture is false, the generator will not generate any fixtures

func NewRustFixtureGenerator(s *suite.Suite, shouldGenerateFixture bool) *RustFixtureGenerator {
	rustFixtureGenerator := &RustFixtureGenerator{
		shouldGenerateFixture: shouldGenerateFixture,
	}

	fixtureName := getTopLevelTestName(s)
	if shouldGenerateFixture {
		s.T().Cleanup(func() {
			s.T().Logf("Writing fixtures for %s", fixtureName)
			if err := rustFixtureGenerator.writeFixtures(fixtureName); err != nil {
				s.T().Logf("Error writing fixtures: %v", err)
			}
		})
	}

	return rustFixtureGenerator
}

// GenerateRustFixture generates a fixture by json marshalling jsonMarshalble and saves it to a file
func (g *RustFixtureGenerator) AddFixtureStep(stepName string, jsonMarshalble interface{}) {
	if !g.shouldGenerateFixture {
		return
	}

	g.fixture.Steps = append(g.fixture.Steps, ethereumtypes.Step{
		Name: stepName,
		Data: jsonMarshalble,
	})
}

func (g *RustFixtureGenerator) ShouldGenerateFixture() bool {
	return g.shouldGenerateFixture
}

func (g *RustFixtureGenerator) writeFixtures(fixtureName string) error {
	if !g.shouldGenerateFixture {
		return nil
	}

	filePath := fmt.Sprintf("%s/%s.json", testvalues.RustFixturesDir, fixtureName)

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
