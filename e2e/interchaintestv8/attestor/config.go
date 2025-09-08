package attestor

import (
	"fmt"
	"os"

	"github.com/BurntSushi/toml"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
)

// ServerConfig represents the server configuration section
type ServerConfig struct {
	Address  string `toml:"address"`
	Port     int    `toml:"port"`
	LogLevel string `toml:"log_level"`
}

// OpConfig represents the OP-specific configuration
type OpConfig struct {
	URL           string `toml:"url"`
	RouterAddress string `toml:"router_address"`
}

// CosmosConfig represents the Cosmos-specific configuration
type CosmosConfig struct {
	URL string `toml:"url"`
}

// AttestorConfig represents the full attestor configuration
type AttestorConfig struct {
	Server ServerConfig `toml:"server"`
	OP     OpConfig     `toml:"op"`
	Cosmos CosmosConfig `toml:"cosmos"`
}

// DefaultAttestorConfig returns a config with sensible defaults
func DefaultAttestorConfig() *AttestorConfig {
	return &AttestorConfig{
		Server: ServerConfig{
			Address:  "0.0.0.0",
			Port:     2025,
			LogLevel: os.Getenv(testvalues.EnvKeyRustLog),
		},
		OP: OpConfig{
			URL:           "https://api.tatum.io/v3/blockchain/node/ethereum-mainnet",
			RouterAddress: "0xa348CfE719B63151F228e3C30EB424BA5a983012",
		},
	}
}

// WriteTomlConfig writes the attestor config to a TOML file
func (c *AttestorConfig) WriteTomlConfig(filePath string) error {
	file, err := os.Create(filePath)
	if err != nil {
		return err
	}
	defer file.Close()

	encoder := toml.NewEncoder(file)
	return encoder.Encode(c)
}

// GetServerAddress returns the server address and port
func (c *AttestorConfig) GetServerAddress() string {
	return fmt.Sprintf("%s:%d", c.Server.Address, c.Server.Port)
}

// GetServerPort returns just the port number
func (c *AttestorConfig) GetServerPort() int {
	return c.Server.Port
}

// GetOpURL returns the OP chain URL
func (c *AttestorConfig) GetOpURL() string {
	return c.OP.URL
}

// CleanupConfig removes the config file
func CleanupConfig(filePath string) error {
	return os.Remove(filePath)
}
