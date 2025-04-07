package cosmos

import (
	"fmt"

	"cosmossdk.io/collections"

	sdk "github.com/cosmos/cosmos-sdk/types"
	banktypes "github.com/cosmos/cosmos-sdk/x/bank/types"

	abcitypes "github.com/cometbft/cometbft/abci/types"
)

// CloneAppend returns a new slice with the contents of the provided slices.
func CloneAppend(bz []byte, tail []byte) (res []byte) {
	res = make([]byte, len(bz)+len(tail))
	copy(res, bz)
	copy(res[len(bz):], tail)
	return
}

// BankBalanceKey returns the store key for a given address and denomination pair.
func BankBalanceKey(addr sdk.AccAddress, denom string) ([]byte, error) {
	keyCodec := collections.PairKeyCodec(sdk.AccAddressKey, collections.StringKey)
	cKey := collections.Join(addr, denom)
	return collections.EncodeKeyWithPrefix(banktypes.BalancesPrefix, keyCodec, cKey)
}

// GetEventValue retrieves the value of a specific attribute from a specific event type in the provided events.
func GetEventValue(events []abcitypes.Event, eventType, attrKey string) (string, error) {
	for _, event := range events {
		if event.Type != eventType {
			continue
		}

		for _, attr := range event.Attributes {
			if attr.Key == attrKey {
				return attr.Value, nil
			}
		}
	}

	return "", fmt.Errorf("event type %s with attribute key %s not found", eventType, attrKey)
}
