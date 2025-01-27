package ics20lib

import (
	"math/big"

	"github.com/ethereum/go-ethereum/accounts/abi"
)

// EncodeFungibleTokenPacketData abi encodes the ICS20Transfer payload data.
// This works the same way as abi.encode(ICS20LibFungibleTokenPacketData) in Solidity.
// The encoded bytes are used as the payload in the Packet data.
func EncodeFungibleTokenPacketData(ftpd ICS20LibFungibleTokenPacketData) ([]byte, error) {
	parsedABI, err := getFungibleTokenPacketDataABI()
	if err != nil {
		return nil, err
	}

	return parsedABI.Pack(ftpd)
}

// DecodeFungibleTokenPacketData decodes an abi encoded ICS20Transfer payload data.
// This works the same way as abi.decode(payload) in Solidity.
func DecodeFungibleTokenPacketData(abiEncodedFtpd []byte) (ICS20LibFungibleTokenPacketData, error) {
	parsedABI, err := getFungibleTokenPacketDataABI()
	if err != nil {
		return ICS20LibFungibleTokenPacketData{}, err
	}

	unpacked, err := parsedABI.Unpack(abiEncodedFtpd)
	if err != nil {
		return ICS20LibFungibleTokenPacketData{}, err
	}

	// We have to do this because Unpack returns a slice of interfaces where the concrete type is an anonymous struct
	decodedAnon := unpacked[0].(struct {
		Tokens []struct {
			Denom struct {
				Base  string `json:"base"`
				Trace []struct {
					PortId    string `json:"portId"`
					ChannelId string `json:"channelId"`
				} `json:"trace"`
			} `json:"denom"`
			Amount *big.Int `json:"amount"`
		} `json:"tokens"`
		Sender     string `json:"sender"`
		Receiver   string `json:"receiver"`
		Memo       string `json:"memo"`
		Forwarding struct {
			DestinationMemo string `json:"destinationMemo"`
			Hops            []struct {
				PortId    string `json:"portId"`
				ChannelId string `json:"channelId"`
			} `json:"hops"`
		} `json:"forwarding"`
	})

	tokens := make([]ICS20LibToken, len(decodedAnon.Tokens))
	for i, token := range decodedAnon.Tokens {
		trace := make([]ICS20LibHop, len(token.Denom.Trace))
		for j, hop := range token.Denom.Trace {
			trace[j] = ICS20LibHop{
				PortId:    hop.PortId,
				ChannelId: hop.ChannelId,
			}
		}

		tokens[i] = ICS20LibToken{
			Denom: ICS20LibDenom{
				Base:  token.Denom.Base,
				Trace: trace,
			},
			Amount: token.Amount,
		}
	}

	forwarding := ICS20LibForwardingPacketData{
		DestinationMemo: decodedAnon.Forwarding.DestinationMemo,
		Hops:            make([]ICS20LibHop, len(decodedAnon.Forwarding.Hops)),
	}
	for i, hop := range decodedAnon.Forwarding.Hops {
		forwarding.Hops[i] = ICS20LibHop{
			PortId:    hop.PortId,
			ChannelId: hop.ChannelId,
		}
	}
	decoded := ICS20LibFungibleTokenPacketData{
		Tokens:     tokens,
		Sender:     decodedAnon.Sender,
		Receiver:   decodedAnon.Receiver,
		Memo:       decodedAnon.Memo,
		Forwarding: forwarding,
	}

	return decoded, nil
}

func getFungibleTokenPacketDataABI() (abi.Arguments, error) {
	parsedABI, err := LibMetaData.GetAbi()
	if err != nil {
		return nil, err
	}

	return parsedABI.Methods["abiPublicTypes"].Inputs, nil
}
