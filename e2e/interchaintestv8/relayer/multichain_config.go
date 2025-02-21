package relayer

import (
	"html/template"
	"os"
)

// MultichainConfigInfo is a struct that holds the configuration information for the multichain config template
type MultichainConfigInfo struct {
	// Chain A chain identifier
	ChainAID string
	// Chain B chain identifier
	ChainBID string
	// Ethereum chain identifier
	EthChainID string
	// Chain A tendermint RPC URL
	ChainATmRPC string
	// Chain B tendermint RPC URL
	ChainBTmRPC string
	// Chain A signer address
	ChainASignerAddress string
	// Chain B signer address
	ChainBSignerAddress string
	// ICS26 Router address
	ICS26Address string
	// Ethereum RPC URL
	EthRPC string
	// Ethereum Beacon API URL
	BeaconAPI string
	// SP1 private key
	SP1PrivateKey string
	// Whether we use the mock client in the cosmos chains
	MockWasmClient bool
	// Whether we use the mock SP1 client
	MockSP1Client bool
}

// GenerateMultichainConfigFile generates a multichain config file from the template.
func (c *MultichainConfigInfo) GenerateMultichainConfigFile(path string) error {
	tmpl, err := template.ParseFiles("e2e/interchaintestv8/relayer/multichain_config.tmpl")
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
