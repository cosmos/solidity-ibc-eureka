package main

import (
	"fmt"
	"math/big"
	"os"
	"time"

	"github.com/cosmos/solidity-ibc-eureka/abigen/ics20transfer"
	ethcommon "github.com/ethereum/go-ethereum/common"
	ethtypes "github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/crypto"
	"github.com/ethereum/go-ethereum/ethclient"
	"github.com/spf13/cobra"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/cmd/utils"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/erc20"
)

func TransferFromEth() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "transfer-from-eth-to-cosmos [amount] [erc20-contract-address] [to-address]", // TODO: Better name???
		Short: "Send a transfer from Ethereum to Cosmos",
		Args:  cobra.ExactArgs(3),
		RunE: func(cmd *cobra.Command, args []string) error {
			ctx := cmd.Context()

			// get args
			amountStr := args[0]
			transferAmount, ok := new(big.Int).SetString(amountStr, 10)
			if !ok {
				return fmt.Errorf("invalid amount: %s", amountStr)
			}
			erc20Address := ethcommon.HexToAddress(args[1])
			to := args[2]

			// get flags
			ethRPC, _ := cmd.Flags().GetString(FlagEthRPC)

			ics20Str, _ := cmd.Flags().GetString(FlagIcs20Address)
			ics20Address := ethcommon.HexToAddress(ics20Str)

			ethPrivateKeyStr := os.Getenv(EnvEthPrivateKey)
			if ethPrivateKeyStr == "" {
				return fmt.Errorf("ETH_PRIVATE_KEY env var not set")
			}

			sourceClientID, _ := cmd.Flags().GetString(FlagSourceClientID)

			// Set up everything needed to send the transfer
			ethClient, err := ethclient.Dial(ethRPC)
			if err != nil {
				return err
			}

			ics20Contract, err := ics20transfer.NewContract(ics20Address, ethClient)

			erc20Contract, err := erc20.NewContract(erc20Address, ethClient)

			ethChainID, err := ethClient.ChainID(ctx)
			ethPrivKey := utils.EthPrivateKeyFromHex(ethPrivateKeyStr)
			ethereumUserAddress := crypto.PubkeyToAddress(ethPrivKey.PublicKey)

			// Approve ICS20 contract to spend ERC20
			// TODO: Consider if we should query Permit2, so we don't have to do this every time 🤔
			tx, err := erc20Contract.Approve(utils.GetTransactOpts(ethClient, ethChainID, ethPrivKey), ics20Address, transferAmount)
			if err != nil {
				return err
			}
			receipt := utils.GetTxReciept(ctx, ethClient, tx.Hash())
			if receipt == nil || receipt.Status != ethtypes.ReceiptStatusSuccessful {
				return fmt.Errorf("approve tx unsuccessful (%s) %+v", tx.Hash().String(), receipt)
			}
			fmt.Printf("Approved ICS20 contract (%s) to spend ERC20 (%s) from %s\n", ics20Address.Hex(), erc20Address.Hex(), ethereumUserAddress.Hex())

			timeout := uint64(time.Now().Add(1 * time.Hour).Unix())
			sendTransferMsg := ics20transfer.IICS20TransferMsgsSendTransferMsg{
				Denom:            erc20Address,
				Amount:           transferAmount,
				Receiver:         to,
				SourceClient:     sourceClientID,
				DestPort:         "transfer",
				TimeoutTimestamp: timeout,
				Memo:             "",
			}
			tx, err = ics20Contract.SendTransfer(utils.GetTransactOpts(ethClient, ethChainID, ethPrivKey), sendTransferMsg)
			if err != nil {
				fmt.Printf("tx %+v\n", tx)
				return fmt.Errorf("send transfer tx unsuccessful\nmsg %+v\nerr: %w", sendTransferMsg, err)
			}
			receipt = utils.GetTxReciept(ctx, ethClient, tx.Hash())
			if receipt == nil || receipt.Status != ethtypes.ReceiptStatusSuccessful {
				return fmt.Errorf("send transfer tx (%s) unsuccessful %+v", tx.Hash().String(), receipt)
			}

			fmt.Printf("Transfer sent from %s to %s of %d %s with tx hash %s\n", ethereumUserAddress.Hex(), to, transferAmount, erc20Address.Hex(), tx.Hash().Hex())

			return nil
		},
	}

	AddEthFlags(cmd)
	cmd.Flags().String(FlagSourceClientID, MockTendermintClientID, "Tendermint Client ID on Ethereum")

	return cmd
}
