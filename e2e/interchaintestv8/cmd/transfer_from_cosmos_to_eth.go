package main

import (
	"fmt"
	"math/big"
	"os"
	"strings"
	"time"

	"cosmossdk.io/log"
	sdkmath "cosmossdk.io/math"
	dbm "github.com/cosmos/cosmos-db"
	"github.com/cosmos/cosmos-sdk/client/tx"
	simtestutil "github.com/cosmos/cosmos-sdk/testutil/sims"
	sdk "github.com/cosmos/cosmos-sdk/types"
	txtypes "github.com/cosmos/cosmos-sdk/types/tx"
	"github.com/cosmos/cosmos-sdk/types/tx/signing"
	xauthsigning "github.com/cosmos/cosmos-sdk/x/auth/signing"
	accounttypes "github.com/cosmos/cosmos-sdk/x/auth/types"
	"github.com/cosmos/ibc-go/modules/light-clients/08-wasm/testing/simapp"
	transfertypes "github.com/cosmos/ibc-go/v10/modules/apps/transfer/types"
	channeltypesv2 "github.com/cosmos/ibc-go/v10/modules/core/04-channel/v2/types"
	ethcommon "github.com/ethereum/go-ethereum/common"
	"github.com/spf13/cobra"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/cmd/utils"
)

func TransferFromCosmos() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "transfer-from-cosmos-to-eth [amount] [denom] [to-ethereum-address]", // TODO: Better name???
		Short: "Send a transfer from Ethereum to Cosmos",
		Args:  cobra.ExactArgs(3),
		RunE: func(cmd *cobra.Command, args []string) error {
			ctx := cmd.Context()

			db := dbm.NewMemDB()
			app := simapp.NewUnitTestSimApp(log.NewNopLogger(), db, nil, true, simtestutil.EmptyAppOptions{}, nil)

			// get args
			amountStr := args[0]
			transferAmount, ok := new(big.Int).SetString(amountStr, 10)
			if !ok {
				return fmt.Errorf("invalid amount: %s", amountStr)
			}
			denom := args[1]
			to := ethcommon.HexToAddress(args[2])

			// get flags
			cosmosPrivateKeyStr := os.Getenv(EnvCosmosPrivateKey)
			if cosmosPrivateKeyStr == "" {
				return fmt.Errorf("%s env var not set", EnvCosmosPrivateKey)
			}
			cosmosPrivateKey, err := utils.CosmosPrivateKeyFromHex(cosmosPrivateKeyStr)
			if err != nil {
				return err
			}

			sourceClientID, _ := cmd.Flags().GetString(FlagSourceClientID)
			if sourceClientID == "" {
				return fmt.Errorf("source client flag not set")
			}

			cosmosRPC, _ := cmd.Flags().GetString(FlagCosmosRPC)
			if cosmosRPC == "" {
				return fmt.Errorf("cosmos-rpc flag not set")
			}

			cosmosChainID, _ := cmd.Flags().GetString(FlagCosmosChainID)
			if cosmosChainID == "" {
				return fmt.Errorf("cosmos-chain-id flag not set")
			}

			cosmosGrpcAddress, _ := cmd.Flags().GetString(FlagCosmosGRPC)
			if cosmosGrpcAddress == "" {
				return fmt.Errorf("cosmos-grpc flag not set")
			}

			// Set up everything needed to send the transfer
			cosmosAddress := sdk.AccAddress(cosmosPrivateKey.PubKey().Address())

			timeout := uint64(time.Now().Add(1 * time.Hour).Unix())
			ibcCoin := sdk.NewCoin(denom, sdkmath.NewIntFromBigInt(transferAmount))

			transferPayload := transfertypes.FungibleTokenPacketData{
				Denom:    ibcCoin.Denom,
				Amount:   ibcCoin.Amount.String(),
				Sender:   cosmosAddress.String(),
				Receiver: strings.ToLower(to.Hex()),
				Memo:     "",
			}
			encodedPayload, err := transfertypes.EncodeABIFungibleTokenPacketData(&transferPayload)
			if err != nil {
				return fmt.Errorf("failed to abi encode payload: %w", err)
			}

			payload := channeltypesv2.Payload{
				SourcePort:      transfertypes.PortID,
				DestinationPort: transfertypes.PortID,
				Version:         transfertypes.V1,
				Encoding:        transfertypes.EncodingABI,
				Value:           encodedPayload,
			}

			msg := &channeltypesv2.MsgSendPacket{
				SourceClient:     sourceClientID,
				TimeoutTimestamp: timeout,
				Payloads: []channeltypesv2.Payload{
					payload,
				},
				Signer: cosmosAddress.String(),
			}

			grpcConn, err := utils.GetTLSGRPC(cosmosGrpcAddress)
			if err != nil {
				return err
			}

			// Get account for sequence and account number
			accountClient := accounttypes.NewQueryClient(grpcConn)
			accountRes, err := accountClient.AccountInfo(ctx, &accounttypes.QueryAccountInfoRequest{Address: cosmosAddress.String()})
			if err != nil {
				return fmt.Errorf("failed to get account info: %w", err)
			}

			txBuilder := app.TxConfig().NewTxBuilder()
			txBuilder.SetGasLimit(200000)
			txBuilder.SetMsgs(msg)

			sigV2 := signing.SignatureV2{
				PubKey: cosmosPrivateKey.PubKey(),
				Data: &signing.SingleSignatureData{
					SignMode:  signing.SignMode(app.TxConfig().SignModeHandler().DefaultMode()),
					Signature: nil,
				},
				Sequence: accountRes.Info.Sequence,
			}
			err = txBuilder.SetSignatures(sigV2)
			if err != nil {
				return fmt.Errorf("failed to set signature: %w", err)
			}

			signerData := xauthsigning.SignerData{
				Address:       cosmosAddress.String(),
				ChainID:       cosmosChainID,
				AccountNumber: accountRes.Info.AccountNumber,
			}
			sigV2, err = tx.SignWithPrivKey(
				ctx,
				signing.SignMode(app.TxConfig().SignModeHandler().DefaultMode()),
				signerData,
				txBuilder,
				cosmosPrivateKey,
				app.TxConfig(),
				accountRes.Info.Sequence,
			)
			if err != nil {
				return fmt.Errorf("failed to sign with priv key: %w", err)
			}
			err = txBuilder.SetSignatures(sigV2)
			if err != nil {
				return fmt.Errorf("failed to set signature: %w", err)
			}

			// Generated Protobuf-encoded bytes.
			txBytes, err := app.TxConfig().TxEncoder()(txBuilder.GetTx())
			if err != nil {
				return fmt.Errorf("failed to encode tx: %w", err)
			}

			txClient := txtypes.NewServiceClient(grpcConn)
			// We then call the BroadcastTx method on this client.
			grpcRes, err := txClient.BroadcastTx(
				ctx,
				&txtypes.BroadcastTxRequest{
					Mode:    txtypes.BroadcastMode_BROADCAST_MODE_SYNC,
					TxBytes: txBytes, // Proto-binary of the signed transaction, see previous step.
				},
			)
			if err != nil {
				return fmt.Errorf("failed to broadcast tx: %w", err)
			}
			if grpcRes.TxResponse.Code != 0 {
				return fmt.Errorf("tx failed with code %d: %+v", grpcRes.TxResponse.Code, grpcRes.TxResponse)
			}

			txResp, err := txClient.GetTx(ctx, &txtypes.GetTxRequest{Hash: grpcRes.TxResponse.TxHash})
			if err != nil {
				return fmt.Errorf("failed to get tx: %w", err)
			}
			if txResp.TxResponse.Code != 0 {
				return fmt.Errorf("tx failed with code %d: %+v", txResp.TxResponse.Code, txResp.TxResponse)
			}

			fmt.Printf("Transfer sent from %s to %s of %d %s with tx hash %s\n", cosmosAddress.String(), to.Hex(), transferAmount, denom, grpcRes.TxResponse.TxHash)
			rpcTxURL := cosmosRPC + "/tx?hash=0x" + grpcRes.TxResponse.TxHash
			fmt.Printf("Find full event logs here: %s\n", rpcTxURL)
			return nil
		},
	}

	AddCosmosFlags(cmd)
	cmd.Flags().String(FlagSourceClientID, MockEthClientID, "Ethereum Client ID on Cosmos")

	return cmd
}
