package gmphelpers

import (
	"github.com/cosmos/gogoproto/proto"

	codectypes "github.com/cosmos/cosmos-sdk/codec/types"

	gmptypes "github.com/cosmos/ibc-go/v10/modules/apps/27-gmp/types"
)

// NewPayload_FromProto creates a new payload to be submitted to cosmos through gmp.
func NewPayload_FromProto(msgs []proto.Message) ([]byte, error) {
	cosmosMsgs := make([]*codectypes.Any, len(msgs))
	for i, msg := range msgs {
		protoAny, err := codectypes.NewAnyWithValue(msg)
		if err != nil {
			return nil, err
		}

		cosmosMsgs[i] = protoAny
	}

	cosmosTx := gmptypes.CosmosTx{
		Messages: cosmosMsgs,
	}
	cosmosTxBz, err := proto.Marshal(&cosmosTx)
	if err != nil {
		return nil, err
	}

	return cosmosTxBz, nil
}
