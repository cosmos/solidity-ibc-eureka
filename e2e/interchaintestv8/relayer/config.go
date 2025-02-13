package relayer

import (
	"os"
	"text/template"
)

// EthCosmosConfigInfo is a struct that holds the configuration information for the Eth to Cosmos config template
type EthCosmosConfigInfo struct {
	// Ethereum chain identifier
	EthChainID string
	// Cosmos chain identifier
	CosmosChainID string
	// Tendermint RPC URL
	TmRPC string
	// ICS26 Router address
	ICS26Address string
	// Ethereum RPC URL
	EthRPC string
	// Ethereum Beacon API URL
	BeaconAPI string
	// SP1 private key
	SP1PrivateKey string
	// Signer address cosmos
	SignerAddress string
	// Whether we use the mock client in Cosmos
	MockWasmClient bool
	// Whether we use the mock SP1 client
	MockSP1Client bool
}

// GenerateEthCosmosConfigFile generates an eth to cosmos config file from the template.
func (c *EthCosmosConfigInfo) GenerateEthCosmosConfigFile(path string) error {
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

// CosmosToCosmosConfigInfo is a struct that holds the configuration information for the Cosmos to Cosmos config template
type CosmosToCosmosConfigInfo struct {
	// Chain A chain identifier
	ChainAID string
	// Chain B chain identifier
	ChainBID string
	// ChainA Tendermint RPC URL
	ChainATmRPC string
	// ChainB Tendermint RPC URL
	ChainBTmRPC string
	// ChainA Submitter address
	ChainAUser string
	// ChainB Submitter address
	ChainBUser string
}

// GenerateCosmosToCosmosConfigFile generates a cosmos to cosmos config file from the template.
func (c *CosmosToCosmosConfigInfo) GenerateCosmosToCosmosConfigFile(path string) error {
	tmpl, err := template.ParseFiles("e2e/interchaintestv8/relayer/cosmos_to_cosmos_config.tmpl")
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

// ChainAToChainBGRPCAddress returns the address for the cosmos to cosmos relayer gRPC server.
func (c *CosmosToCosmosConfigInfo) ChainAToChainBGRPCAddress() string {
	return "127.0.0.1:3001"
}

// ChainBToChainAGRPCAddress returns the address for the cosmos to cosmos relayer gRPC server.
func (c *CosmosToCosmosConfigInfo) ChainBToChainAGRPCAddress() string {
	return "127.0.0.1:3002"
}
