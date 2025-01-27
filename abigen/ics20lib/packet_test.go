package ics20lib_test

import (
	"math/big"
	"testing"

	"github.com/cosmos/solidity-ibc-eureka/abigen/ics20lib"
	"github.com/stretchr/testify/require"
)

// TODO: Add this back
// const solidityEncodedHex = "000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000000e0000000000000000000000000000000000000000000000000000000000000012000000000000000000000000000000000000000000000000000000000000f4240000000000000000000000000000000000000000000000000000000000000016000000000000000000000000000000000000000000000000000000000000000057561746f6d000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000673656e64657200000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000008726563656976657200000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000046d656d6f00000000000000000000000000000000000000000000000000000000"

func TestEncodeFungibleTokenPacketData(t *testing.T) {
	packetData := ics20lib.ICS20LibFungibleTokenPacketData{
		Tokens: []ics20lib.ICS20LibToken{
			{
				Denom: ics20lib.ICS20LibDenom{
					Base: "uatom",
					Trace: []ics20lib.ICS20LibHop{
						{
							PortId:    "portid",
							ChannelId: "channelid",
						},
					},
				},
				Amount: big.NewInt(100),
			},
		},
		Sender:   "somesender",
		Receiver: "somereceiver",
		Memo:     "somememo",
		Forwarding: ics20lib.ICS20LibForwardingPacketData{
			DestinationMemo: "destinationmemo",
			Hops: []ics20lib.ICS20LibHop{
				{
					PortId:    "portid",
					ChannelId: "channelid",
				},
			},
		},
	}

	encoded, err := ics20lib.EncodeFungibleTokenPacketData(packetData)
	require.NoError(t, err)

	decoded, err := ics20lib.DecodeFungibleTokenPacketData(encoded)
	require.NoError(t, err)

	require.Equal(t, packetData, decoded)
}

// func TestDecodeFungibleTokenPacketData(t *testing.T) {
// 	encodedData, err := hex.DecodeString(solidityEncodedHex)
// 	require.NoError(t, err)
//
// 	decoded, err := ics20lib.DecodeFungibleTokenPacketData(encodedData)
// 	require.NoError(t, err)
//
// 	expectedData := ics20lib.ICS20LibFungibleTokenPacketData{
// 		Denom:    "uatom",
// 		Amount:   big.NewInt(1000000),
// 		Sender:   "sender",
// 		Receiver: "receiver",
// 		Memo:     "memo",
// 	}
// 	require.Equal(t, expectedData, decoded)
// }
