package main

import (
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"strconv"

	ibcwasmtypes "github.com/cosmos/ibc-go/modules/light-clients/08-wasm/v10/types"
	clienttypes "github.com/cosmos/ibc-go/v10/modules/core/02-client/types"
	"github.com/cosmos/solidity-ibc-eureka/abigen/ics26router"
	ethcommon "github.com/ethereum/go-ethereum/common"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/chainconfig"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/ethereum"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
	ethereumtypes "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/ethereum"
)

const (
	EthAPIURL              = "https://ethereum-sepolia-rpc.publicnode.com"
	BeaconAPIURL           = "http://sepolia-geth.orb.local:9596"
	IbcContractAddress     = "0x34fE3b64308b7259860Ad05105e97988Dd72AdE2"
	ChainID                = 11155111
	EtheruemClientChecksum = "eb7b9a762f8664dab81f58f4a792f167812b228a1aa5d1e6bf2c808e8ff14399"
)

func main() {
	ethClient, err := ethereum.NewEthAPI(EthAPIURL)
	if err != nil {
		panic(err)
	}
	beaconAPI := ethereum.NewBeaconAPIClient(BeaconAPIURL)

	genesis, err := beaconAPI.GetGenesis()
	if err != nil {
		panic(err)
	}

	fmt.Printf("Genesis: %+v\n", genesis)
	spec, err := beaconAPI.GetSpec()
	if err != nil {
		panic(err)
	}

	finalizedBlock, err := beaconAPI.GetFinalizedBlocks()
	if err != nil {
		panic(err)
	}
	fmt.Printf("Finalized block: %+v\n", finalizedBlock)

	finalizedSlotInt, err := strconv.Atoi(finalizedBlock.Data.Message.Slot)
	if err != nil {
		panic(err)
	}
	finalizedSlot := uint64(finalizedSlotInt)
	fmt.Printf("Execution height: %d\n", finalizedSlot)
	// finalizedSlotHex := fmt.Sprintf("0x%x", finalizedSlot)

	ethClientState := ethereumtypes.ClientState{
		ChainID:                      ChainID,
		GenesisValidatorsRoot:        ethcommon.Bytes2Hex(genesis.GenesisValidatorsRoot[:]),
		MinSyncCommitteeParticipants: 1,
		GenesisTime:                  uint64(genesis.GenesisTime.Unix()),
		ForkParameters:               spec.ToForkParameters(),
		SecondsPerSlot:               uint64(spec.SecondsPerSlot.Seconds()),
		SlotsPerEpoch:                spec.SlotsPerEpoch,
		EpochsPerSyncCommitteePeriod: spec.EpochsPerSyncCommitteePeriod,
		LatestSlot:                   finalizedSlot,
		IsFrozen:                     false,
		IbcCommitmentSlot:            testvalues.IbcCommitmentSlotHex,
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
		RevisionHeight: finalizedSlot,
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

	executionHeight := finalizedBlock.Data.Message.Body.ExecutionPayload.BlockHash
	proofOfIBCContract, err := ethClient.GetProof(IbcContractAddress, []string{ics26router.IbcStoreStorageSlot}, executionHeight)
	if err != nil {
		panic(err)
	}
	fmt.Printf("Proof of IBC contract: %+v\n", proofOfIBCContract)

	header, err := beaconAPI.GetHeader(strconv.Itoa(int(finalizedSlot)))
	if err != nil {
		panic(err)
	}
	fmt.Printf("Header: %+v\n", header)

	bootstrap, err := beaconAPI.GetBootstrap(header.Root)
	if err != nil {
		panic(err)
	}
	fmt.Printf("Bootstrap: %+v\n", bootstrap)

	if bootstrap.Data.Header.Beacon.Slot != finalizedSlot {
		panic(fmt.Sprintf("creating client: expected exec height %d, to equal boostrap slot %d", finalizedSlot, bootstrap.Data.Header.Beacon.Slot))
	}

	unixTimestamp := bootstrap.Data.Header.Execution.Timestamp

	currentPeriod := finalizedSlot / spec.Period()
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
		Timestamp:            unixTimestamp,
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
