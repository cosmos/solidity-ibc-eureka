package main

import (
	"fmt"

	"github.com/spf13/cobra"
)

const (
	FlagEthRPC    = "eth-rpc"
	DefaultEthRPC = "https://ethereum-sepolia-rpc.publicnode.com"

	FlagIcs26Address    = "ics26-address"
	DefaultIcs26Address = "0x718AbdD2f29A6aC1a34A3e20Dae378B5d3d2B0E9"

	FlagIcs20Address    = "ics20-address"
	DefaultIcs20Address = "0xE80DC519EE86146057B9dBEfBa900Edd7a2385e4"

	FlagErc20Address    = "erc20-address"
	DefaultErc20Address = "0xA4ff49eb6E2Ea77d7D8091f1501385078642603f"

	FlagCosmosRPC    = "cosmos-rpc"
	DefaultCosmosRPC = "https://eureka-devnet-02-node-01-rpc.dev.skip.build:443"

	FlagCosmosGRPC    = "cosmos-grpc"
	DefaultCosmosGRPC = "eureka-devnet-02-node-01-grpc.dev.skip.build:443"

	FlagCosmosChainID    = "cosmos-chain-id"
	DefaultCosmosChainID = "eureka-hub-dev-5"

	FlagEthChainID    = "ethereum-chain-id"
	DefaultEthChainID = "11155111"

	FlagSourceClientID      = "source-client-id"
	FlagCosmosClientIDOnEth = "client-id-on-eth"
	FlagEthClientIDOnCosmos = "client-id-on-cosmos"

	// TODO: Add the non-mock versions of these
	MockTendermintClientID = "hub-devnet-sp1-g16-0"
	MockEthClientID        = "08-wasm-4"

	EnvEthPrivateKey    = "ETH_PRIVATE_KEY"
	EnvCosmosPrivateKey = "COSMOS_PRIVATE_KEY"

	RelayerURL = "eureka-devnet-02-relayer-01.dev.skip.build:443"

	EnvRelayerWallet = "RELAYER_WALLET"

	FlagVerbose = "verbose"

	FlagTransferWithCallbacksMemo = "transfer-with-callbacks-memo"
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

	rootCmd.PersistentFlags().BoolP(FlagVerbose, "v", false, "verbose output")

	return rootCmd
}

func AddEthFlags(cmd *cobra.Command) {
	cmd.Flags().String(FlagEthRPC, DefaultEthRPC, "Ethereum RPC URL")
	cmd.Flags().String(FlagIcs26Address, DefaultIcs26Address, "ICS26 contract address")
	cmd.Flags().String(FlagIcs20Address, DefaultIcs20Address, "ICS20 contract address")
	cmd.Flags().String(FlagErc20Address, DefaultErc20Address, "ERC20 contract address")
}

func AddCosmosFlags(cmd *cobra.Command) {
	cmd.Flags().String(FlagCosmosRPC, DefaultCosmosRPC, "Cosmos RPC URL")
	cmd.Flags().String(FlagCosmosGRPC, DefaultCosmosGRPC, "Cosmos gRPC URL")
	cmd.Flags().String(FlagCosmosChainID, DefaultCosmosChainID, "Cosmos Chain ID")
}

func IsVerbose(cmd *cobra.Command) bool {
	verbose, _ := cmd.Flags().GetBool(FlagVerbose)
	return verbose
}
