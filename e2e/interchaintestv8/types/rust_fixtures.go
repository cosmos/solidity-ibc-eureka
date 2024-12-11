package types

import (
	"encoding/json"
	"fmt"
	"os"
	"strings"

	"github.com/stretchr/testify/suite"

	clienttypes "github.com/cosmos/ibc-go/v9/modules/core/02-client/types"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
	ethereumtypes "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/ethereum"
)

type InitialStateFixture struct {
	ClientState    ethereumtypes.ClientState    `json:"client_state"`
	ConsensusState ethereumtypes.ConsensusState `json:"consensus_state"`
}

type CommitmentProofFixture struct {
	Path           []byte                       `json:"path"`
	StorageProof   ethereumtypes.StorageProof   `json:"storage_proof"`
	ProofHeight    clienttypes.Height           `json:"proof_height"`
	ClientState    ethereumtypes.ClientState    `json:"client_state"`
	ConsensusState ethereumtypes.ConsensusState `json:"consensus_state"`
}

type UpdateClientFixture struct {
	ClientState    ethereumtypes.ClientState    `json:"client_state"`
	ConsensusState ethereumtypes.ConsensusState `json:"consensus_state"`
	Updates        []ethereumtypes.Header       `json:"updates"`
}

type Step struct {
	Name string      `json:"name"`
	Data interface{} `json:"data"`
}

type RustFixture struct {
	Steps []Step `json:"steps"`
}

type RustFixtureGenerator struct {
	shouldGenerateFixture bool

	fixture RustFixture
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

	g.fixture.Steps = append(g.fixture.Steps, Step{
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
