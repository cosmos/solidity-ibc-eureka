package main

import (
	"fmt"
	"math/big"
	"strings"

	banktypes "github.com/cosmos/cosmos-sdk/x/bank/types"
	"github.com/cosmos/solidity-ibc-eureka/abigen/ics20transfer"
	ethcommon "github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/ethclient"
	"github.com/spf13/cobra"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/cmd/utils"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/erc20"
)

func BalanceCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "balance [address] [optional-denom-or-erc20-address]",
		Short: "Get the balance of an address",
		Args:  cobra.MinimumNArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			address := args[0]

			var err error
			if strings.HasPrefix(address, "0x") {
				// Ethereum address
				err = printEtheruemBalance(cmd, address, args)
			} else {
				// Cosmos address
				err = printCosmosBalance(cmd, address, args)
			}

			return err
		},
	}

	AddEthFlags(cmd)
	AddCosmosFlags(cmd)

	return cmd
}

func printEtheruemBalance(cmd *cobra.Command, address string, args []string) error {
	ethAddress := ethcommon.HexToAddress(address)

	ethRPC, _ := cmd.Flags().GetString(FlagEthRPC)
	if ethRPC == "" {
		return fmt.Errorf("eth rpc flag not set")
	}
	ethClient, err := ethclient.Dial(ethRPC)
	if err != nil {
		return err
	}

	erc20Str, _ := cmd.Flags().GetString(FlagErc20Address)
	erc20Address := ethcommon.HexToAddress(erc20Str)

	if len(args) > 1 && strings.HasPrefix(args[1], "0x") {
		erc20Address = ethcommon.HexToAddress(args[1])
	} else if len(args) > 1 {

		ics20Str, _ := cmd.Flags().GetString(FlagIcs20Address)
		if ics20Str == "" {
			return fmt.Errorf("ics20 address flag not set")
		}
		ics20Address := ethcommon.HexToAddress(ics20Str)
		ics20Contract, err := ics20transfer.NewContract(ics20Address, ethClient)
		if err != nil {
			return fmt.Errorf("failed to connect to ICS20 contract: %w", err)
		}

		erc20Address, err = ics20Contract.IbcERC20Contract(nil, args[1])
		if err != nil {
			return fmt.Errorf("failed to get IBC ERC20 contract: %w", err)
		}

	}

	erc20Contract, err := erc20.NewContract(erc20Address, ethClient)
	if err != nil {
		return fmt.Errorf("failed to connect to ERC20 contract: %w", err)
	}

	balance, err := erc20Contract.BalanceOf(nil, ethAddress)
	if err != nil {
		return fmt.Errorf("failed to get balance: %w", err)
	}

	fmt.Printf("%s: %s\n", erc20Address.String(), balance)

	// Print ETH balance
	ethBalance, err := ethClient.BalanceAt(cmd.Context(), ethAddress, nil)
	if err != nil {
		return fmt.Errorf("failed to get ETH balance: %w", err)
	}

	fmt.Printf("ETH: %s\n", (new(big.Rat).Quo(new(big.Rat).SetInt(ethBalance), new(big.Rat).SetInt64(1e18))).FloatString(18))

	return nil
}

func printCosmosBalance(cmd *cobra.Command, address string, args []string) error {
	cosmosGrpcAddress, _ := cmd.Flags().GetString(FlagCosmosGRPC)
	if cosmosGrpcAddress == "" {
		return fmt.Errorf("cosmos-grpc flag not set")
	}

	grpcConn, err := utils.GetGRPC(cosmosGrpcAddress)
	if err != nil {
		return err
	}
	bankQueryClient := banktypes.NewQueryClient(grpcConn)

	if len(args) > 1 {
		resp, err := bankQueryClient.Balance(cmd.Context(), &banktypes.QueryBalanceRequest{Address: address, Denom: args[1]})
		if err != nil {
			return fmt.Errorf("failed to get balance: %w", err)
		}

		return utils.PrintBalance(cmd.Context(), grpcConn, *resp.Balance)
	} else {
		resp, err := bankQueryClient.AllBalances(cmd.Context(), &banktypes.QueryAllBalancesRequest{Address: address})
		if err != nil {
			return fmt.Errorf("failed to get balance: %w", err)
		}

		fmt.Printf("Fetched all balances (%d) for address: %s\n", len(resp.Balances), address)
		for _, balance := range resp.Balances {
			if err := utils.PrintBalance(cmd.Context(), grpcConn, balance); err != nil {
				return err
			}
		}
	}

	return nil
}
