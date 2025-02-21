package main

import (
	"os"

	"fmt"

	ethcommon "github.com/ethereum/go-ethereum/common"
	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials"

	"cosmossdk.io/log"
	dbm "github.com/cosmos/cosmos-db"
	"github.com/cosmos/cosmos-sdk/client/tx"
	simtestutil "github.com/cosmos/cosmos-sdk/testutil/sims"
	sdk "github.com/cosmos/cosmos-sdk/types"
	txtypes "github.com/cosmos/cosmos-sdk/types/tx"
	"github.com/cosmos/cosmos-sdk/types/tx/signing"
	xauthsigning "github.com/cosmos/cosmos-sdk/x/auth/signing"
	accounttypes "github.com/cosmos/cosmos-sdk/x/auth/types"
	"github.com/cosmos/gogoproto/proto"
	"github.com/cosmos/ibc-go/modules/light-clients/08-wasm/testing/simapp"
	"github.com/spf13/cobra"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/cmd/utils"
	relayertypes "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/relayer"
)

func RelayTxCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "relay_tx [txHash]",
		Short: "Relay a transaction (currently only from eth to cosmos)",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			fmt.Printf("Relaying tx with hash %s\n", args[0])
			if err := os.Chdir("../../"); err != nil {
				return err
			}
			ctx := cmd.Context()

			db := dbm.NewMemDB()
			app := simapp.NewSimApp(log.NewNopLogger(), db, nil, true, simtestutil.EmptyAppOptions{}, nil)

			verbose, _ := cmd.Flags().GetBool(FlagVerbose)

			if verbose {
				fmt.Println("App created")
			}

			// Get args
			txHashHexStr := args[0]

			// Get flags
			// Get Relayer Private Key
			cosmosRelayerPrivateKeyStr := os.Getenv(EnvRelayerWallet)
			if cosmosRelayerPrivateKeyStr == "" {
				return fmt.Errorf("%s env var not set", EnvRelayerWallet)
			}
			// Get flags
			cosmosRelayerPrivateKey, err := utils.CosmosPrivateKeyFromHex(cosmosRelayerPrivateKeyStr)
			if err != nil {
				return err
			}

			cosmosRPC, _ := cmd.Flags().GetString(FlagCosmosRPC)
			if cosmosRPC == "" {
				return fmt.Errorf("cosmos-rpc flag not set")
			}
			ethRPC, _ := cmd.Flags().GetString(FlagEthRPC)
			if ethRPC == "" {
				return fmt.Errorf("eth-rpc flag not set")
			}
			ethBeaconURL, _ := cmd.Flags().GetString(FlagEthBeaconURL)
			if ethBeaconURL == "" {
				return fmt.Errorf("eth-beacon-url flag not set")
			}
			cosmosChainID, _ := cmd.Flags().GetString(FlagCosmosChainID)
			if cosmosChainID == "" {
				return fmt.Errorf("cosmos-chain-id flag not set")
			}
			ics26AddressStr, _ := cmd.Flags().GetString(FlagIcs26Address)
			if ics26AddressStr == "" {
				return fmt.Errorf("ics26-address flag not set")
			}
			targetClientID, _ := cmd.Flags().GetString(FlagTargetClientID)
			if targetClientID == "" {
				return fmt.Errorf("target-client-id flag not set")
			}

			// Set up everything we need to relay
			cosmosAddress := sdk.AccAddress(cosmosRelayerPrivateKey.PubKey().Address())

			grpcConn, err := GetCosmosGRPC(cmd)
			if err != nil {
				return err
			}

			txHash := ethcommon.HexToHash(txHashHexStr)

			if verbose {
				fmt.Println("Eth and cosmos setup completed, connecting to relayer on", RelayerURL)
			}

			relayerClient, err := GetTLSGRPCClient(RelayerURL)
			if err != nil {
				return err
			}

			if verbose {
				fmt.Println("Relayer client connected, relaying tx...")
			}

			resp, err := relayerClient.RelayByTx(ctx, &relayertypes.RelayByTxRequest{
				SrcChain:       "0x1",
				DstChain:       cosmosChainID,
				SourceTxIds:    [][]byte{txHash.Bytes()},
				TargetClientId: targetClientID,
			})
			if err != nil {
				return fmt.Errorf("failed to relayed tx: %w", err)
			}

			// Extract messages from the response (cosmos specific)
			var txBody txtypes.TxBody
			if err := proto.Unmarshal(resp.Tx, &txBody); err != nil {
				return err
			}

			if len(txBody.Messages) == 0 {
				return fmt.Errorf("no messages to relay")
			}

			var msgs []sdk.Msg
			for _, msg := range txBody.Messages {
				var sdkMsg sdk.Msg
				if err := app.InterfaceRegistry().UnpackAny(msg, &sdkMsg); err != nil {
					return err
				}

				if verbose {
					fmt.Printf("Relayed message: %+v\n", sdkMsg)
				}

				msgs = append(msgs, sdkMsg)
			}

			// Get account for sequence and account number
			accountClient := accounttypes.NewQueryClient(grpcConn)
			accountRes, err := accountClient.AccountInfo(ctx, &accounttypes.QueryAccountInfoRequest{Address: cosmosAddress.String()})
			if err != nil {
				return err
			}

			txBuilder := app.TxConfig().NewTxBuilder()
			txBuilder.SetGasLimit(200000)
			txBuilder.SetMsgs(msgs...)

			sigV2 := signing.SignatureV2{
				PubKey: cosmosRelayerPrivateKey.PubKey(),
				Data: &signing.SingleSignatureData{
					SignMode:  signing.SignMode(app.TxConfig().SignModeHandler().DefaultMode()),
					Signature: nil,
				},
				Sequence: accountRes.Info.Sequence,
			}
			err = txBuilder.SetSignatures(sigV2)
			if err != nil {
				return err
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
				cosmosRelayerPrivateKey,
				app.TxConfig(),
				accountRes.Info.Sequence,
			)
			if err != nil {
				return err
			}
			err = txBuilder.SetSignatures(sigV2)
			if err != nil {
				return err
			}

			// Generated Protobuf-encoded bytes.
			txBytes, err := app.TxConfig().TxEncoder()(txBuilder.GetTx())
			if err != nil {
				return err
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
				return err
			}
			if grpcRes.TxResponse.Code != 0 {
				return fmt.Errorf("tx failed with code %d: %+v", grpcRes.TxResponse.Code, grpcRes.TxResponse)
			}

			fmt.Printf("Tx relayed successfully with hash %s\n", grpcRes.TxResponse.TxHash)
			rpcTxURL := cosmosRPC + "/tx?hash=0x" + grpcRes.TxResponse.TxHash
			fmt.Printf("Find full event logs here: %s\n", rpcTxURL)
			if verbose {
				for _, event := range grpcRes.TxResponse.Events {
					fmt.Printf("Event: %+v\n", event)
				}
			}

			return nil
		},
	}

	AddEthFlags(cmd)
	AddCosmosFlags(cmd)
	cmd.Flags().String(FlagTargetClientID, MockEthClientID, "Ethereum Client ID on Cosmos")

	return cmd
}

// GetGRPCClient returns a gRPC client for the relayer.
func GetTLSGRPCClient(addr string) (relayertypes.RelayerServiceClient, error) {
	creds := credentials.NewTLS(nil)

	// Establish a secure connection with the gRPC server
	conn, err := grpc.Dial(addr, grpc.
		WithTransportCredentials(creds))
	if err != nil {
		return nil, err
	}

	return relayertypes.NewRelayerServiceClient(conn), nil
}
