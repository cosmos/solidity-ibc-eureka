package main

import (
	"encoding/hex"
	"fmt"
	"os"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/chainconfig"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/ethereum"

	ibcwasmtypes "github.com/cosmos/ibc-go/modules/light-clients/08-wasm/types"
	clienttypes "github.com/cosmos/ibc-go/v9/modules/core/02-client/types"
)

func main() {
	// TODO: Take these values from env or flags or something
	ethRPC := "https://ethereum-sepolia-rpc.publicnode.com"
	wasmClientChecksumHex := "3b8e524cf9db2cb6abe03b45c03c74744eadb97416e574ca3c7a5b8daba0ba7e"

	ethAPI, err := ethereum.NewEthAPI(ethRPC)
	if err != nil {
		panic(err)
	}

	_, ethHeight, err := ethAPI.GetBlockNumber()
	if err != nil {
		panic(err)
	}

	wasmClientChecksum, err := hex.DecodeString(wasmClientChecksumHex)
	if err != nil {
		panic(err)
	}

	latestHeight := clienttypes.Height{
		RevisionNumber: 0,
		RevisionHeight: ethHeight,
	}
	clientState := ibcwasmtypes.ClientState{
		Data:         []byte("doesnt matter"),
		Checksum:     wasmClientChecksum,
		LatestHeight: latestHeight,
	}

	clientStateAny, err := clienttypes.PackClientState(&clientState)
	if err != nil {
		panic(err)
	}

	consensusState := ibcwasmtypes.ConsensusState{
		Data: []byte("doesnt matter"),
	}
	consensusStateAny, err := clienttypes.PackConsensusState(&consensusState)

	consensusJSON, err := chainconfig.Codec().MarshalJSON(consensusStateAny)
	if err != nil {
		panic(err)
	}
	clientJSON, err := chainconfig.Codec().MarshalJSON(clientStateAny)
	if err != nil {
		panic(err)
	}

	// Write to file
	consensusFile, err := os.Create("consensus_state.json")
	if err != nil {
		panic(err)
	}
	defer consensusFile.Close()
	_, err = consensusFile.Write(consensusJSON)
	if err != nil {
		panic(err)
	}
	fmt.Println("Consensus state written to consensus_state.json")

	clientFile, err := os.Create("client_state.json")
	if err != nil {
		panic(err)
	}
	defer clientFile.Close()
	_, err = clientFile.Write(clientJSON)
	if err != nil {
		panic(err)
	}
	fmt.Println("Client state written to client_state.json")
}
