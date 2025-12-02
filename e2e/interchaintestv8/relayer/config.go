package relayer

import (
	"encoding/json"
	"fmt"
	"os"
	"text/template"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
)

const (
	ModuleCosmosToCosmos    = "cosmos_to_cosmos"
	ModuleCosmosToEth       = "cosmos_to_eth"
	ModuleEthToCosmos       = "eth_to_cosmos"
	ModuleEthToCosmosCompat = "eth_to_cosmos_compat"
)

// Config represents the relayer configuration structure aligned with the Rust RelayerConfig
// and serves as template data for generation in e2e.
type Config struct {
	Modules       []ModuleConfig      `json:"modules"`
	Server        ServerConfig        `json:"server"`
	Observability ObservabilityConfig `json:"observability"`
}

// ServerConfig mirrors the Rust ServerConfig
type ServerConfig struct {
	Address string `json:"address"`
	Port    int    `json:"port"`
}

// ObservabilityConfig mirrors the Rust ObservabilityConfig
type ObservabilityConfig struct {
	Level        string  `json:"level"`
	UseOtel      bool    `json:"use_otel"`
	ServiceName  string  `json:"service_name"`
	OTelEndpoint *string `json:"otel_endpoint,omitempty"`
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

// NewConfig creates a new Config with default values
func NewConfig(modules []ModuleConfig) Config {
	// Server defaults
	server := ServerConfig{
		Address: "127.0.0.1",
		Port:    3000,
	}

	// Observability configuration
	rustLog := os.Getenv(testvalues.EnvKeyRustLog)
	if rustLog == "" {
		rustLog = testvalues.EnvValueRustLog_Info
	}

	// Local observability flag strictly equals "true"
	enableLocalObservability := os.Getenv(testvalues.EnvKeyEnableLocalObservability) == testvalues.EnvValueEnableLocalObservability_True

	var otlpEndpoint *string
	tracingLevel := rustLog
	useOtel := false
	if enableLocalObservability {
		// Force the endpoint and enable OTLP, but respect RUST_LOG for level
		endpoint := "http://127.0.0.1:4317"
		otlpEndpoint = &endpoint
		useOtel = true
	}

	observability := ObservabilityConfig{
		Level:        tracingLevel,
		UseOtel:      useOtel,
		ServiceName:  "ibc-eureka-relayer",
		OTelEndpoint: otlpEndpoint,
	}

	return Config{
		Modules:       modules,
		Server:        server,
		Observability: observability,
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
