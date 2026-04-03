package gmphelpers

import (
	"fmt"

	codectypes "github.com/cosmos/cosmos-sdk/codec/types"
	"github.com/cosmos/gogoproto/proto"
	gmptypes "github.com/cosmos/ibc-go/v10/modules/apps/27-gmp/types"
	"github.com/ethereum/go-ethereum/accounts/abi"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
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

// UnmarshalAck decodes a GMP acknowledgement using the appropriate encoding.
func UnmarshalAck(data []byte, encoding string) (gmptypes.Acknowledgement, error) {
	if encoding == testvalues.Ics27AbiEncoding {
		return DecodeABIAck(data)
	}

	var ack gmptypes.Acknowledgement
	if err := proto.Unmarshal(data, &ack); err != nil {
		return gmptypes.Acknowledgement{}, fmt.Errorf("unmarshalling protobuf ack: %w", err)
	}

	return ack, nil
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
