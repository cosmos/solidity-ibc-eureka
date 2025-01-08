package relayer

import (
	"fmt"
	"os"
	"text/template"
)

type EthCosmosConfigInfo struct {
	// gRPC port for the Eth to Cosmos relayer module
	EthToCosmosPort uint64
	// gRPC port for the Cosmos to Eth relayer module
	CosmosToEthPort uint64
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
	Mock bool
}

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

// EthToCosmosGRPCAddress returns the address for the eth to cosmos relayer gRPC server.
func (c *EthCosmosConfigInfo) EthToCosmosGRPCAddress() string {
	return fmt.Sprintf("127.0.0.1:%d", c.EthToCosmosPort)
}

// CosmosToEthGRPCAddress returns the address for the eth to cosmos relayer gRPC server.
func (c *EthCosmosConfigInfo) CosmosToEthGRPCAddress() string {
	return fmt.Sprintf("127.0.0.1:%d", c.CosmosToEthPort)
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
