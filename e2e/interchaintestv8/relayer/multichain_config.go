package relayer

import (
	"fmt"
	"html/template"
	"os"
)

// MultichainConfigInfo is a struct that holds the configuration information for the multichain config template
type MultichainConfigInfo struct {
	// gRPC port for the Eth to Chain A relayer module
	EthToChainAPort uint64
	// gRPC port for the Chain A to Eth relayer module
	ChainAToEthPort uint64
	// gRPC port for the Eth to Chain B relayer module
	EthToChainBPort uint64
	// gRPC port for the Chain B to Eth relayer module
	ChainBToEthPort uint64
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
	Mock bool
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

// EthToChainAGRPCAddress returns the address for the eth to chain A relayer gRPC server.
func (c *MultichainConfigInfo) EthToChainAGRPCAddress() string {
	return fmt.Sprintf("127.0.0.1:%d", c.EthToChainAPort)
}

// ChainAToEthGRPCAddress returns the address for the chain A to eth relayer gRPC server.
func (c *MultichainConfigInfo) ChainAToEthGRPCAddress() string {
	return fmt.Sprintf("127.0.0.1:%d", c.ChainAToEthPort)
}

// EthToChainAGRPCAddress returns the address for the eth to chain A relayer gRPC server.
func (c *MultichainConfigInfo) EthToChainBGRPCAddress() string {
	return fmt.Sprintf("127.0.0.1:%d", c.EthToChainBPort)
}

// ChainAToEthGRPCAddress returns the address for the chain B to eth relayer gRPC server.
func (c *MultichainConfigInfo) ChainBToEthGRPCAddress() string {
	return fmt.Sprintf("127.0.0.1:%d", c.ChainBToEthPort)
}
