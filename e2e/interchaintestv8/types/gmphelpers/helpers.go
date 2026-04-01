package gmphelpers

import (
	"fmt"

	"github.com/cosmos/gogoproto/proto"
	"github.com/ethereum/go-ethereum/accounts/abi"

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

// DecodeABIAck decodes an ABI-encoded GMP acknowledgement.
func DecodeABIAck(data []byte) (gmptypes.Acknowledgement, error) {
	tupleType, err := abi.NewType("tuple", "", []abi.ArgumentMarshaling{
		{Name: "result", Type: "bytes"},
	})
	if err != nil {
		return gmptypes.Acknowledgement{}, fmt.Errorf("creating abi type: %w", err)
	}

	args := abi.Arguments{{Type: tupleType}}
	unpacked, err := args.Unpack(data)
	if err != nil {
		return gmptypes.Acknowledgement{}, fmt.Errorf("unpacking abi ack: %w", err)
	}

	parsed, ok := unpacked[0].(struct {
		Result []byte `json:"result"`
	})
	if !ok {
		return gmptypes.Acknowledgement{}, fmt.Errorf("unexpected abi ack type")
	}

	return gmptypes.Acknowledgement{Result: parsed.Result}, nil
}
