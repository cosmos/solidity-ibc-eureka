package main

import (
	"context"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"

	ibcwasmtypes "github.com/cosmos/ibc-go/modules/light-clients/08-wasm/v10/types"
	clienttypes "github.com/cosmos/ibc-go/v10/modules/core/02-client/types"
	"github.com/cosmos/solidity-ibc-eureka/go-abigen/ics26router"
	ethcommon "github.com/ethereum/go-ethereum/common"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/chainconfig"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/ethereum"
	ethereumtypes "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/ethereum"
)

const (
	EthAPIURL              = "https://ethereum-rpc.publicnode.com"
	BeaconAPIURL           = ""
	IbcContractAddress     = ""
	ChainID                = 11155111
	EtheruemClientChecksum = "77dd083b93f4d7d7199aa432a04f33109a1692af2483d87c689577540fca4fbc"
)

func main() {
	ctx := context.Background()
	ethClient, err := ethereum.NewEthAPI(EthAPIURL)
	if err != nil {
		panic(err)
	}
	beaconAPI, err := ethereum.NewBeaconAPIClient(ctx, BeaconAPIURL)
	if err != nil {
		panic(err)
	}

	genesis, err := beaconAPI.GetGenesis()
	if err != nil {
		panic(err)
	}

	fmt.Printf("Genesis: %+v\n", genesis)
	spec, err := beaconAPI.GetSpec()
	if err != nil {
		panic(err)
	}

	beaconBlock, err := beaconAPI.GetBeaconBlocks("finalized")
	if err != nil {
		panic(err)
	}
	executionHeight := beaconBlock.Data.Message.Body.ExecutionPayload.BlockNumber
	executionNumberHex := fmt.Sprintf("0x%x", executionHeight)

	header, err := beaconAPI.GetHeader(beaconBlock.Data.Message.Slot)
	if err != nil {
		panic(err)
	}
	bootstrap, err := beaconAPI.GetBootstrap(header.Root)
	if err != nil {
		panic(err)
	}
	if bootstrap.Data.Header.Execution.BlockNumber != executionHeight {
		panic(fmt.Sprintf("creating client: expected exec height %d, to equal boostrap block number %d", executionHeight, bootstrap.Data.Header.Execution.BlockNumber))
	}

	latestSlot := bootstrap.Data.Header.Beacon.Slot
	fmt.Printf("Latest slot: %+v\n", latestSlot)

	ethClientState := ethereumtypes.ClientState{
		ChainID:                      ChainID,
		GenesisValidatorsRoot:        ethcommon.Bytes2Hex(genesis.GenesisValidatorsRoot[:]),
		MinSyncCommitteeParticipants: 1,
		GenesisTime:                  uint64(genesis.GenesisTime.Unix()),
		GenesisSlot:                  spec.GenesisSlot,
		ForkParameters:               spec.ToForkParameters(),
		SecondsPerSlot:               uint64(spec.SecondsPerSlot.Seconds()),
		SlotsPerEpoch:                spec.SlotsPerEpoch,
		EpochsPerSyncCommitteePeriod: spec.EpochsPerSyncCommitteePeriod,
		LatestSlot:                   latestSlot,
		IsFrozen:                     false,
		IbcCommitmentSlot:            ics26router.IbcStoreStorageSlot,
		IbcContractAddress:           IbcContractAddress,
	}

	ethClientStateBz, err := json.Marshal(&ethClientState)
	if err != nil {
		panic(err)
	}
	fmt.Printf("Eth Client state: %+v\n", ethClientState)

	wasmClientChecksum, err := hex.DecodeString(EtheruemClientChecksum)
	if err != nil {
		panic(err)
	}

	latestHeightSlot := clienttypes.Height{
		RevisionNumber: 0,
		RevisionHeight: latestSlot,
	}
	clientState := ibcwasmtypes.ClientState{
		Data:         ethClientStateBz,
		Checksum:     wasmClientChecksum,
		LatestHeight: latestHeightSlot,
	}
	clientStateAny, err := clienttypes.PackClientState(&clientState)
	if err != nil {
		panic(err)
	}
	clientJSON, err := chainconfig.Codec().MarshalJSON(clientStateAny)
	if err != nil {
		panic(err)
	}

	fmt.Printf("Wasm Client state: %s\n", clientJSON)

	proofOfIBCContract, err := ethClient.GetProof(IbcContractAddress, []string{ics26router.IbcStoreStorageSlot}, executionNumberHex)
	if err != nil {
		panic(err)
	}
	fmt.Printf("Proof of IBC contract: %+v\n", proofOfIBCContract)

	currentPeriod := latestSlot / spec.Period()
	clientUpdates, err := beaconAPI.GetLightClientUpdates(currentPeriod, 1)
	if err != nil {
		panic(err)
	}
	if len(clientUpdates) == 0 {
		panic("no client updates")
	}

	ethConsensusState := ethereumtypes.ConsensusState{
		Slot:                 bootstrap.Data.Header.Beacon.Slot,
		StateRoot:            bootstrap.Data.Header.Execution.StateRoot,
		StorageRoot:          proofOfIBCContract.StorageHash,
		Timestamp:            bootstrap.Data.Header.Execution.Timestamp,
		CurrentSyncCommittee: bootstrap.Data.CurrentSyncCommittee.AggregatePubkey,
		NextSyncCommittee:    clientUpdates[0].Data.NextSyncCommittee.AggregatePubkey,
	}

	ethConsensusStateBz, err := json.Marshal(&ethConsensusState)
	if err != nil {
		panic(err)
	}
	consensusState := ibcwasmtypes.ConsensusState{
		Data: ethConsensusStateBz,
	}
	consensusStateAny, err := clienttypes.PackConsensusState(&consensusState)
	if err != nil {
		panic(err)
	}

	consensusJSON, err := chainconfig.Codec().MarshalJSON(consensusStateAny)
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
