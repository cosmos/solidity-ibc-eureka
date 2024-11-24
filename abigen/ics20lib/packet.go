package ics20lib

import (
	"math/big"

	"github.com/ethereum/go-ethereum/accounts/abi"
)

func EncodeFungibleTokenPacketData(ftpd ICS20LibFungibleTokenPacketData) ([]byte, error) {
	parsedABI, err := getFungibleTokenPacketDataABI()
	if err != nil {
		return nil, err
	}

	return parsedABI.Pack(ftpd)
}

func DecodeFungibleTokenPacketData(bz []byte) (ICS20LibFungibleTokenPacketData, error) {
	parsedABI, err := getFungibleTokenPacketDataABI()
	if err != nil {
		return ICS20LibFungibleTokenPacketData{}, err
	}

	unpacked, err := parsedABI.Unpack(bz)
	if err != nil {
		return ICS20LibFungibleTokenPacketData{}, err
	}

	// We have to do this because Unpack returns a slice of interfaces where the concrete type is an anonymous struct
	decodedAnon := unpacked[0].(struct {
		Denom    string   `json:"denom"`
		Sender   string   `json:"sender"`
		Receiver string   `json:"receiver"`
		Amount   *big.Int `json:"amount"`
		Memo     string   `json:"memo"`
	})
	decoded := ICS20LibFungibleTokenPacketData{
		Denom:    decodedAnon.Denom,
		Amount:   decodedAnon.Amount,
		Sender:   decodedAnon.Sender,
		Receiver: decodedAnon.Receiver,
		Memo:     decodedAnon.Memo,
	}

	return decoded, nil
}

func getFungibleTokenPacketDataABI() (abi.Arguments, error) {
	parsedABI, err := LibMetaData.GetAbi()
	if err != nil {
		return nil, err
	}

	return parsedABI.Methods["decodePayload"].Outputs, nil
}
