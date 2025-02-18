package main

import (
	"os"

	"fmt"

	ethcommon "github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/ethclient"

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
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/relayer"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
	relayertypes "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/relayer"
	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"
)

func RelayTxCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "relay_tx [txHash]",
		Short: "Relay a transaction (currently only from eth to cosmos)",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			if err := os.Chdir("../../"); err != nil {
				return err
			}
			ctx := cmd.Context()

			db := dbm.NewMemDB()
			app := simapp.NewSimApp(log.NewNopLogger(), db, nil, true, simtestutil.EmptyAppOptions{}, nil)

			// Get args
			txHashHexStr := args[0]

			// Get flags
			cosmosPrivateKeyStr := os.Getenv(EnvCosmosPrivateKey)
			if cosmosPrivateKeyStr == "" {
				return fmt.Errorf("%s env var not set", EnvCosmosPrivateKey)
			}
			cosmosPrivateKey, err := utils.CosmosPrivateKeyFromHex(cosmosPrivateKeyStr)
			if err != nil {
				return err
			}

			cosmosRPC, _ := cmd.Flags().GetString(FlagCosmosRPC)
			cosmosGrpcAddress, _ := cmd.Flags().GetString(FlagCosmosGRPC)
			ethRPC, _ := cmd.Flags().GetString(FlagEthRPC)
			ethBeaconURL, _ := cmd.Flags().GetString(FlagEthBeaconURL)
			cosmosChainID, _ := cmd.Flags().GetString(FlagCosmosChainID)
			ics26AddressStr, _ := cmd.Flags().GetString(FlagIcs26Address)
			targetClientID, _ := cmd.Flags().GetString(FlagTargetClientID)

			// Set up everything we need to relay
			cosmosAddress := sdk.AccAddress(cosmosPrivateKey.PubKey().Address())

			grpcConn, err := grpc.Dial(
				cosmosGrpcAddress,
				grpc.WithTransportCredentials(insecure.NewCredentials()),
			)
			if err != nil {
				return err
			}
			defer grpcConn.Close()

			txHash := ethcommon.HexToHash(txHashHexStr)

			ethClient, err := ethclient.Dial(ethRPC)
			if err != nil {
				return err
			}

			ethChainID, err := ethClient.ChainID(ctx)

			configInfo := relayer.EthCosmosConfigInfo{
				EthChainID:     ethChainID.String(),
				CosmosChainID:  cosmosChainID,
				TmRPC:          cosmosRPC,
				ICS26Address:   ics26AddressStr,
				EthRPC:         ethRPC,
				BeaconAPI:      ethBeaconURL,
				SP1PrivateKey:  "", // for now
				SignerAddress:  cosmosAddress.String(),
				MockWasmClient: true, // for now
				MockSP1Client:  true,
			}
			if err := configInfo.GenerateEthCosmosConfigFile(testvalues.RelayerConfigFilePath); err != nil {
				return err
			}
			defer func() {
				os.Remove(testvalues.RelayerConfigFilePath)
			}()

			relayerProcess, err := relayer.StartRelayer(testvalues.RelayerConfigFilePath)
			if err != nil {
				return err
			}
			defer relayerProcess.Kill()

			relayerClient, err := relayer.GetGRPCClient(relayer.DefaultRelayerGRPCAddress())
			if err != nil {
				return err
			}

			resp, err := relayerClient.RelayByTx(ctx, &relayertypes.RelayByTxRequest{
				SrcChain:       ethChainID.String(),
				DstChain:       cosmosChainID,
				SourceTxIds:    [][]byte{txHash.Bytes()},
				TargetClientId: targetClientID,
			})
			if err != nil {
				return err
			}

			// Extract messages from the response (cosmos specific)
			var txBody txtypes.TxBody
			if err := proto.Unmarshal(resp.Tx, &txBody); err != nil {
				return err
			}

			var msgs []sdk.Msg
			for _, msg := range txBody.Messages {
				var sdkMsg sdk.Msg
				if err := app.InterfaceRegistry().UnpackAny(msg, &sdkMsg); err != nil {
					return err
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
				PubKey: cosmosPrivateKey.PubKey(),
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
				cosmosPrivateKey,
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

			return nil
		},
	}
}
