package relayer

import (
	"os"
	"text/template"
)

// ConfigInfo is a struct that holds the configuration information for the config template
type ConfigInfo struct {
	// Tendermint RPC URL
	TmRPC string
	// ICS26 Router address
	ICS26Address string
	// Ethereum RPC URL
	EthRPC string
	// SP1 private key
	SP1PrivateKey string
}

func (c *ConfigInfo) GenerateConfigFile(path string) error {
	tmpl, err := template.ParseFiles("e2e/interchaintestv8/relayer/config.tmpl")
	if err != nil {
		return err
	}

	f, err := os.Create(path)
	if err != nil {
		return err
	}

	defer f.Close()

	return tmpl.Execute(f, c)
}
