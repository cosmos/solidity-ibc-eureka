package main

import (
	"fmt"

	"github.com/spf13/cobra"
)

const (
	FlagEthRPC    = "eth-rpc"
	DefaultEthRPC = "https://ethereum-sepolia-rpc.publicnode.com"

	FlagEthBeaconURL    = "eth-beacon-url"
	DefaultEthBeaconURL = "https://ethereum-sepolia-beacon-api.publicnode.com"

	FlagIcs26Address    = "ics26-address"
	DefaultIcs26Address = "0x15cB0fC94d072B367a1A2D7f0c8fF9792aB9f546"

	FlagIcs20Address    = "ics20-address"
	DefaultIcs20Address = "0xbb87C1ACc6306ad2233a4c7BBE75a1230409b358"

	FlagErc20Address    = "erc20-address"
	DefaultErc20Address = "0xA4ff49eb6E2Ea77d7D8091f1501385078642603f"

	FlagCosmosRPC    = "cosmos-rpc"
	DefaultCosmosRPC = "https://eureka-devnet-02-node-01-rpc.dev.skip.build:443"

	FlagCosmosGRPC    = "cosmos-grpc"
	DefaultCosmosGRPC = "eureka-devnet-02-node-01-grpc.dev.skip.build:443"

	FlagCosmosChainID    = "cosmos-chain-id"
	DefaultCosmosChainID = "eureka-dev-3"

	FlagSourceClientID      = "source-client-id"
	FlagCosmosClientIDOnEth = "client-id-on-eth"
	FlagEthClientIDOnCosmos = "client-id-on-cosmos"

	MockTendermintClientID = "client-6"
	MockEthClientID        = "08-wasm-0"
	// TODO: Add the non-mock versions of these

	EnvEthPrivateKey    = "ETH_PRIVATE_KEY"
	EnvCosmosPrivateKey = "COSMOS_PRIVATE_KEY"

	RelayerURL = "eureka-devnet-02-relayer-01.dev.skip.build:443"

	EnvRelayerWallet = "RELAYER_WALLET"
)

func main() {
	if err := RootCmd().Execute(); err != nil {
		fmt.Println("Something went wrong!")
		fmt.Printf("Error: %+v\n", err)
	}
}

func RootCmd() *cobra.Command {
	rootCmd := &cobra.Command{
		Use:   "eureka-cli",
		Short: "IBC Eureka CLI",
	}

	rootCmd.AddCommand(TransferFromEth())
	rootCmd.AddCommand(RelayTxCmd())
	rootCmd.AddCommand(BalanceCmd())
	rootCmd.AddCommand(TransferFromCosmos())

	return rootCmd
}

func AddEthFlags(cmd *cobra.Command) {
	cmd.Flags().String(FlagEthRPC, DefaultEthRPC, "Ethereum RPC URL")
	cmd.Flags().String(FlagEthBeaconURL, DefaultEthBeaconURL, "Ethereum Beacon URL")
	cmd.Flags().String(FlagIcs26Address, DefaultIcs26Address, "ICS26 contract address")
	cmd.Flags().String(FlagIcs20Address, DefaultIcs20Address, "ICS20 contract address")
	cmd.Flags().String(FlagErc20Address, DefaultErc20Address, "ERC20 contract address")
}

func AddCosmosFlags(cmd *cobra.Command) {
	cmd.Flags().String(FlagCosmosRPC, DefaultCosmosRPC, "Cosmos RPC URL")
	cmd.Flags().String(FlagCosmosGRPC, DefaultCosmosGRPC, "Cosmos gRPC URL")
	cmd.Flags().String(FlagCosmosChainID, DefaultCosmosChainID, "Cosmos Chain ID")
}
