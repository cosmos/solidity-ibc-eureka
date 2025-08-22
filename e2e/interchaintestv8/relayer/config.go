package relayer

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"text/template"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
)

const (
	ModuleCosmosToCosmos    = "cosmos_to_cosmos"
	ModuleCosmosToEth       = "cosmos_to_eth"
	ModuleEthToCosmos       = "eth_to_cosmos"
	ModuleEthToCosmosCompat = "eth_to_cosmos_compat"
)

// Config represents the relayer configuration structure and serves as template data
type Config struct {
	LogLevel string         `json:"log_level"`
	Address  string         `json:"address"`
	Port     int            `json:"port"`
	Modules  []ModuleConfig `json:"modules"`
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
	return Config{
		LogLevel: os.Getenv(testvalues.EnvKeyRustLog),
		Address:  "127.0.0.1",
		Port:     3000,
		Modules:  modules,
	}
}

// GenerateConfig creates a config from the template
func (c *Config) GenerateConfigFile(filePath string) error {
	// Debug logging to understand working directory and file path resolution
	cwd, err := os.Getwd()
	if err != nil {
		fmt.Printf("WARNING: Failed to get current working directory: %v\n", err)
	} else {
		fmt.Printf("DEBUG: Current working directory: %s\n", cwd)
	}
	
	templatePath := "relayer/config.tmpl"
	fmt.Printf("DEBUG: Attempting to parse template file: %s\n", templatePath)
	
	// Check if template file exists
	if _, err := os.Stat(templatePath); os.IsNotExist(err) {
		fmt.Printf("DEBUG: Template file does not exist at: %s\n", templatePath)
		return fmt.Errorf("template file not found at: %s", templatePath)
	} else {
		fmt.Printf("DEBUG: Template file exists at: %s\n", templatePath)
	}
	
	tmpl, err := template.ParseFiles(templatePath)
	if err != nil {
		fmt.Printf("DEBUG: Failed to parse template file '%s': %v\n", templatePath, err)
		return err
	}

	// Ensure the directory exists before creating the file
	dir := filepath.Dir(filePath)
	fmt.Printf("DEBUG: Creating directory if needed: %s\n", dir)
	err = os.MkdirAll(dir, 0755)
	if err != nil {
		fmt.Printf("DEBUG: Failed to create directory '%s': %v\n", dir, err)
		return fmt.Errorf("failed to create directory '%s': %w", dir, err)
	}
	
	fmt.Printf("DEBUG: Creating config file at: %s\n", filePath)
	f, err := os.Create(filePath)
	if err != nil {
		fmt.Printf("DEBUG: Failed to create config file '%s': %v\n", filePath, err)
		return fmt.Errorf("failed to create config file '%s': %w", filePath, err)
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
