package types

import (
	"encoding/json"
	"fmt"
	"os"

	clienttypes "github.com/cosmos/ibc-go/v9/modules/core/02-client/types"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
	ethereumlightclient "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/ethereumlightclient"
)

type CommitmentProofFixture struct {
	Path           []byte                             `json:"path"`
	StorageProof   ethereumlightclient.StorageProof   `json:"storage_proof"`
	ProofHeight    clienttypes.Height                 `json:"proof_height"`
	ClientState    ethereumlightclient.ClientState    `json:"client_state"`
	ConsensusState ethereumlightclient.ConsensusState `json:"consensus_state"`
}

type RustFixtureGenerator struct {
	shouldGenerateFixture bool
	prefix                string

	// fixtureCount is used to create a clear order of fixtures
	fixtureCount uint
}

// NewRustFixtureGenerator creates a new RustFixtureGenerator
// If shouldGenerateFixture is false, the generator will not generate any fixtures
func NewRustFixtureGenerator(prefix string, shouldGenerateFixture bool) *RustFixtureGenerator {
	return &RustFixtureGenerator{
		prefix:                prefix,
		shouldGenerateFixture: shouldGenerateFixture,
	}
}

// GenerateRustFixture generates a fixture by json marshalling jsonMarshalble and saves it to a file
func (g *RustFixtureGenerator) GenerateRustFixture(name string, jsonMarshalble interface{}) error {
	fixturesBz, err := json.MarshalIndent(jsonMarshalble, "", "  ")
	if err != nil {
		return err
	}

	g.fixtureCount++

	fixtureName := fmt.Sprintf("%s_%d_%s", g.prefix, g.fixtureCount, name)
	filePath := fmt.Sprintf("%s/%s.json", testvalues.RustFixturesDir, fixtureName)

	return os.WriteFile(filePath, fixturesBz, 0644)
}

func (g *RustFixtureGenerator) ShouldGenerateFixture() bool {
	return g.shouldGenerateFixture
}
