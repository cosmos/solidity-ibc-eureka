package ics20lib

import (
	"math/big"

	"github.com/ethereum/go-ethereum/accounts/abi"
)

// EncodeFungibleTokenPacketDataV2 abi encodes the ICS20Transfer payload data.
// This works the same way as abi.encode(IICS20TransferMsgsFungibleTokenPacketDataV2) in Solidity.
// The encoded bytes are used as the payload in the Packet data.
func EncodeFungibleTokenPacketDataV2(ftpd IICS20TransferMsgsFungibleTokenPacketDataV2) ([]byte, error) {
	parsedABI, err := getFungibleTokenPacketDataABI()
	if err != nil {
		return nil, err
	}

	return parsedABI.Pack(ftpd)
}

// DecodeFungibleTokenPacketDataV2 decodes an abi encoded ICS20Transfer payload data.
// This works the same way as abi.decode(payload) in Solidity.
func DecodeFungibleTokenPacketDataV2(abiEncodedFtpd []byte) (IICS20TransferMsgsFungibleTokenPacketDataV2, error) {
	parsedABI, err := getFungibleTokenPacketDataABI()
	if err != nil {
		return IICS20TransferMsgsFungibleTokenPacketDataV2{}, err
	}

	unpacked, err := parsedABI.Unpack(abiEncodedFtpd)
	if err != nil {
		return IICS20TransferMsgsFungibleTokenPacketDataV2{}, err
	}

	// We have to do this because Unpack returns a slice of interfaces where the concrete type is an anonymous struct
	decodedAnon := unpacked[0].(struct {
		Tokens []struct {
			Denom struct {
				Base  string `json:"base"`
				Trace []struct {
					PortId   string `json:"portId"`
					ClientId string `json:"clientId"`
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
				PortId   string `json:"portId"`
				ClientId string `json:"clientId"`
			} `json:"hops"`
		} `json:"forwarding"`
	})

	tokens := make([]IICS20TransferMsgsToken, len(decodedAnon.Tokens))
	for i, token := range decodedAnon.Tokens {
		trace := make([]IICS20TransferMsgsHop, len(token.Denom.Trace))
		for j, hop := range token.Denom.Trace {
			trace[j] = IICS20TransferMsgsHop{
				PortId:   hop.PortId,
				ClientId: hop.ClientId,
			}
		}

		tokens[i] = IICS20TransferMsgsToken{
			Denom: IICS20TransferMsgsDenom{
				Base:  token.Denom.Base,
				Trace: trace,
			},
			Amount: token.Amount,
		}
	}

	forwarding := IICS20TransferMsgsForwardingPacketData{
		DestinationMemo: decodedAnon.Forwarding.DestinationMemo,
		Hops:            make([]IICS20TransferMsgsHop, len(decodedAnon.Forwarding.Hops)),
	}
	for i, hop := range decodedAnon.Forwarding.Hops {
		forwarding.Hops[i] = IICS20TransferMsgsHop{
			PortId:   hop.PortId,
			ClientId: hop.ClientId,
		}
	}
	decoded := IICS20TransferMsgsFungibleTokenPacketDataV2{
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
