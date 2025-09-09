package relayer

import (
	"encoding/json"
	"errors"
	"fmt"
	"math/rand"
	"net"
	"os"
	"text/template"
	"time"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
)

const (
	ModuleCosmosToCosmos      = "cosmos_to_cosmos"
	ModuleCosmosToEth         = "cosmos_to_eth"
	ModuleCosmosToEthAttested = "cosmos_to_eth_attested"
	ModuleEthToCosmos         = "eth_to_cosmos"
	ModuleEthToCosmosCompat   = "eth_to_cosmos_compat"
	ModuleEthToCosmosAttested = "eth_to_cosmos_attested"
)

// Config represents the relayer configuration structure and serves as template data
type Config struct {
	LogLevel    string         `json:"log_level"`
	Address     string         `json:"address"`
	Port        int            `json:"port"`
	GrpcWebPort int            `json:"grpc_web_port"`
	Modules     []ModuleConfig `json:"modules"`
}

// ModuleConfig represents a module configuration
type ModuleConfig struct {
	Name     string `json:"name"`
	SrcChain string `json:"src_chain"`
	DstChain string `json:"dst_chain"`
	Config   any    `json:"config"`
}

// SP1ProverConfig represents the configuration for SP1 prover
type SP1ProverConfig struct {
	Type              string `json:"type"`
	NetworkPrivateKey string `json:"network_private_key,omitempty"`
	NetworkRpcUrl     string `json:"network_rpc_url,omitempty"`
	PrivateCluster    bool   `json:"private_cluster,omitempty"`
}

type SP1ProgramPaths struct {
	UpdateClient              string `json:"update_client"`
	Membership                string `json:"membership"`
	UpdateClientAndMembership string `json:"update_client_and_membership"`
	Misbehaviour              string `json:"misbehaviour"`
}

// CosmosToEthModuleConfig represents the configuration for cosmos_to_eth module
type CosmosToEthModuleConfig struct {
	TmRpcUrl     string          `json:"tm_rpc_url"`
	Ics26Address string          `json:"ics26_address"`
	EthRpcUrl    string          `json:"eth_rpc_url"`
	Sp1Prover    SP1ProverConfig `json:"sp1_prover"`
	Sp1Programs  SP1ProgramPaths `json:"sp1_programs"`
}

// CosmosToCosmosModuleConfig represents the configuration for cosmos_to_cosmos module
type CosmosToCosmosModuleConfig struct {
	SrcRpcUrl     string `json:"src_rpc_url"`
	TargetRpcUrl  string `json:"target_rpc_url"`
	SignerAddress string `json:"signer_address"`
}

// GetAvailablePort returns a random available TCP port on localhost within a typical ephemeral range.
// It tries up to maxAttempts times to find a free port by binding and immediately releasing it.
func GetAvailablePort() (int, error) {
	const (
		minPort     = 20000
		maxPort     = 40000
		maxAttempts = 50
	)
	// Seed RNG once per process
	rand.Seed(time.Now().UnixNano())
	for i := 0; i < maxAttempts; i++ {
		p := rand.Intn(maxPort-minPort+1) + minPort
		ln, err := net.Listen("tcp", fmt.Sprintf("127.0.0.1:%d", p))
		if err == nil {
			_ = ln.Close()
			return p, nil
		}
	}
	return 0, errors.New("failed to find an available port")
}

// NewConfig creates a new Config with default values
func NewConfig(modules []ModuleConfig) Config {
	addr := "127.0.0.1"
	port, err := GetAvailablePort()
	if err != nil {
		// Fallback to a sensible default if no port found; tests may still override via env
		port = 3000
	}
	grpcWebPort, err := GetAvailablePort()
	if err != nil {
		grpcWebPort = 8081
	}
	// Ensure ports are distinct
	if grpcWebPort == port {
		if alt, err := GetAvailablePort(); err == nil {
			grpcWebPort = alt
		} else {
			grpcWebPort = port + 1
		}
	}

	// Make the chosen address visible to tests that use DefaultRelayerGRPCAddress
	_ = os.Setenv("RELAYER_GRPC_ADDR", fmt.Sprintf("%s:%d", addr, port))

	return Config{
		LogLevel:    os.Getenv(testvalues.EnvKeyRustLog),
		Address:     addr,
		Port:        port,
		GrpcWebPort: grpcWebPort,
		Modules:     modules,
	}
}

// GenerateConfig creates a config from the template
func (c *Config) GenerateConfigFile(filePath string) error {
	tmpl, err := template.ParseFiles("e2e/interchaintestv8/relayer/config.tmpl")
	if err != nil {
		return err
	}

	f, err := os.Create(filePath)
	if err != nil {
		return err
	}
	defer f.Close()

	return tmpl.Execute(f, c)
}

// ToJSON converts a ModuleConfig to JSON string - designed for use in templates
func (mc ModuleConfig) ToJSON() string {
	jsonBytes, err := json.MarshalIndent(mc, "", "  ")
	if err != nil {
		return fmt.Sprintf("\"Error generating JSON: %s\"", err)
	}
	return string(jsonBytes)
}

// ModulesToJSON converts a slice of ModuleConfig to a JSON string
func ModulesToJSON(modules []ModuleConfig) (string, error) {
	if len(modules) == 0 {
		return "[]", nil
	}

	jsonBytes, err := json.MarshalIndent(modules, "", "  ")
	if err != nil {
		return "", fmt.Errorf("failed to marshal modules to JSON: %w", err)
	}

	return string(jsonBytes), nil
}

// ethToCosmosConfig represents the configuration for eth_to_cosmos module
type ethToCosmosConfig struct {
	TmRpcUrl        string `json:"tm_rpc_url"`
	Ics26Address    string `json:"ics26_address"`
	EthRpcUrl       string `json:"eth_rpc_url"`
	EthBeaconApiUrl string `json:"eth_beacon_api_url"`
	SignerAddress   string `json:"signer_address"`
	Mock            bool   `json:"mock"`
}

// EthToCosmosAttestedModuleConfig represents the configuration for attested_to_cosmos module
type EthToCosmosAttestedModuleConfig struct {
	AttestedChainId  string           `json:"attested_chain_id"`
	AggregatorConfig AggregatorConfig `json:"aggregator_config"`
	AttestedRpcUrl   string           `json:"attested_rpc_url"`
	Ics26Address     string           `json:"ics26_address"`
	TmRpcUrl         string           `json:"tm_rpc_url"`
	SignerAddress    string           `json:"signer_address"`
}

// CosmosToEthAttestedModuleConfig represents the configuration for cosmos_to_eth_attested module
type CosmosToEthAttestedModuleConfig struct {
	AttestedChainId  string           `json:"attested_chain_id"`
	AggregatorConfig AggregatorConfig `json:"aggregator_config"`
	AttestedRpcUrl   string           `json:"attested_rpc_url"`
	Ics26Address     string           `json:"ics26_address"`
	EthRpcUrl        string           `json:"eth_rpc_url"`
}
