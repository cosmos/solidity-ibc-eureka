package relayer

import (
	"encoding/json"
	"fmt"
	"os"
	"text/template"
)

// Config represents the relayer configuration structure and serves as template data
type Config struct {
	LogLevel string         `json:"log_level"`
	Address  string         `json:"address"`
	Port     int            `json:"port"`
	Modules  []ModuleConfig `json:"modules"`
}

// NewConfig creates a new Config with default values
func NewConfig(modules []ModuleConfig) Config {
	return Config{
		LogLevel: "info",
		Address:  "127.0.0.1",
		Port:     3000,
		Modules:  modules,
	}
}

// ModuleConfig represents a module configuration
type ModuleConfig struct {
	Name     string `json:"name"`
	SrcChain string `json:"src_chain"`
	DstChain string `json:"dst_chain"`
	Config   any    `json:"config"`
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

// CosmosToCosmosConfig represents the configuration for cosmos_to_cosmos module
type CosmosToCosmosConfig struct {
	SrcRpcUrl     string `json:"src_rpc_url"`
	TargetRpcUrl  string `json:"target_rpc_url"`
	SignerAddress string `json:"signer_address"`
}

// SP1ProverType represents the type of SP1 prover
type SP1ProverType string

const (
	ModuleCosmosToCosmos               = "cosmos_to_cosmos"
	ModuleCosmosToEth                  = "cosmos_to_eth"
	ModuleEthToCosmos                  = "eth_to_cosmos"
	SP1ProverMock        SP1ProverType = "mock"
	SP1ProverEnv         SP1ProverType = "env"
	SP1ProverNetwork     SP1ProverType = "network"
	SP1ProverCpu         SP1ProverType = "cpu"
	SP1ProverCuda        SP1ProverType = "cuda"
)

// SP1Config represents the configuration for SP1 prover
type SP1Config struct {
	ProverType        SP1ProverType `json:"prover_type"`
	NetworkPrivateKey string        `json:"network_private_key,omitempty"`
	NetworkRpcUrl     string        `json:"network_rpc_url,omitempty"`
	PrivateCluster    bool          `json:"private_cluster,omitempty"`
}

// CosmosToEthConfig represents the configuration for cosmos_to_eth module
type CosmosToEthConfig struct {
	TmRpcUrl     string    `json:"tm_rpc_url"`
	Ics26Address string    `json:"ics26_address"`
	EthRpcUrl    string    `json:"eth_rpc_url"`
	Sp1Config    SP1Config `json:"sp1_config"`
}

// EthToCosmosConfig represents the configuration for eth_to_cosmos module
type EthToCosmosConfig struct {
	TmRpcUrl        string `json:"tm_rpc_url"`
	Ics26Address    string `json:"ics26_address"`
	EthRpcUrl       string `json:"eth_rpc_url"`
	EthBeaconApiUrl string `json:"eth_beacon_api_url"`
	SignerAddress   string `json:"signer_address"`
	Mock            bool   `json:"mock"`
}

func CreateEthCosmosModules(
	ethChainID string,
	cosmosChainID string,
	tmRpcUrl string,
	mockWasmClient bool,
	cosmosSignerAddress string,
	ethRpcUrl string,
	ethBeaconApiUrl string,
	ics26Address string,
	sp1Config SP1Config,
) []ModuleConfig {
	return []ModuleConfig{
		{
			Name:     ModuleEthToCosmos,
			SrcChain: ethChainID,
			DstChain: cosmosChainID,
			Config: EthToCosmosConfig{
				TmRpcUrl:        tmRpcUrl,
				Ics26Address:    ics26Address,
				EthRpcUrl:       ethRpcUrl,
				EthBeaconApiUrl: ethBeaconApiUrl,
				SignerAddress:   cosmosSignerAddress,
				Mock:            mockWasmClient,
			},
		},
		{
			Name:     ModuleCosmosToEth,
			SrcChain: cosmosChainID,
			DstChain: ethChainID,
			Config: CosmosToEthConfig{
				TmRpcUrl:     tmRpcUrl,
				Ics26Address: ics26Address,
				EthRpcUrl:    ethRpcUrl,
				Sp1Config:    sp1Config,
			},
		},
	}
}

func CreateCosmosCosmosModules(
	chainAID string,
	chainBID string,
	chainARpcUrl string,
	chainBRpcUrl string,
	chainASingerAddress string,
	chainBSingerAddress string,
) []ModuleConfig {
	return []ModuleConfig{
		{
			Name:     ModuleCosmosToCosmos,
			SrcChain: chainAID,
			DstChain: chainBID,
			Config: CosmosToCosmosConfig{
				SrcRpcUrl:     chainARpcUrl,
				TargetRpcUrl:  chainBRpcUrl,
				SignerAddress: chainBSingerAddress,
			},
		},
		{
			Name:     ModuleCosmosToCosmos,
			SrcChain: chainBID,
			DstChain: chainAID,
			Config: CosmosToCosmosConfig{
				SrcRpcUrl:     chainBRpcUrl,
				TargetRpcUrl:  chainARpcUrl,
				SignerAddress: chainASingerAddress,
			},
		},
	}
}

// GenerateConfig creates a config from the template
func (c *Config) GenerateConfigFile(filePath string) error {
	tmpl, err := template.ParseFiles("e2e/interchaintestv8/relayer/config.tmpl")
	if err != nil {
		return err
	}

	f, err := os.Create(filePath)
	defer f.Close()

	return tmpl.Execute(f, c)
}
