package utils

import (
	"context"
	"encoding/hex"
	"fmt"
	"strings"

	"github.com/cosmos/cosmos-sdk/crypto/keys/secp256k1"
	cryptotypes "github.com/cosmos/cosmos-sdk/crypto/types"
	sdk "github.com/cosmos/cosmos-sdk/types"
	transfertypes "github.com/cosmos/ibc-go/v10/modules/apps/transfer/types"
	"google.golang.org/grpc"
)

func CosmosPrivateKeyFromHex(hexKey string) (cryptotypes.PrivKey, error) {
	keyBytes, err := hex.DecodeString(hexKey)
	if err != nil {
		return nil, fmt.Errorf("invalid key string: %w", err)
	}
	privKey := &secp256k1.PrivKey{Key: keyBytes}
	privKey.PubKey().Address()
	return privKey, nil
}

func PrintBalance(ctx context.Context, grpcConn *grpc.ClientConn, coin sdk.Coin) error {
	denom := coin.Denom
	if strings.HasPrefix(coin.Denom, "ibc/") {
		// IBC token
		transferQueryClient := transfertypes.NewQueryV2Client(grpcConn)
		resp, err := transferQueryClient.Denom(ctx, &transfertypes.QueryDenomRequest{Hash: coin.Denom})
		if err != nil {
			return fmt.Errorf("failed to query IBC denom: %w", err)
		}

		fmt.Printf("IBC Denom: %s\n", coin.Denom)
		denom = resp.Denom.Path()
	}

	fmt.Printf("%s: %s\n", denom, coin.Amount)
	return nil
}
