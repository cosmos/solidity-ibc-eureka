package relayer

import (
	"os"
	"text/template"
)

// EthToCosmosConfigInfo is a struct that holds the configuration information for the Ethereum to Cosmos config template
type EthToCosmosConfigInfo struct {
	// Tendermint RPC URL
	TmRPC string
	// ICS26 Router address
	ICS26Address string
	// Ethereum RPC URL
	EthRPC string
	// SP1 private key
	SP1PrivateKey string
}

func (c *EthToCosmosConfigInfo) GenerateEthToCosmosConfigFile(path string) error {
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

// EthToCosmosGRPCAddress returns the address for the eth to cosmos relayer gRPC server.
func (c *EthToCosmosConfigInfo) EthToCosmosGRPCAddress() string {
	return "127.0.0.1:3000"
}

// CosmosToCosmosConfigInfo is a struct that holds the configuration information for the Cosmos to Cosmos config template
type CosmosToCosmosConfigInfo struct {
	// ChainA Tendermint RPC URL
	ChainATmRPC string
	// ChainB Tendermint RPC URL
	ChainBTmRPC string
	// ChainA Submitter address
	ChainAUser string
	// ChainB Submitter address
	ChainBUser string
}

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
