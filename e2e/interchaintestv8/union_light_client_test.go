package main

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"math/big"
	"os"
	"strconv"
	"testing"
	"time"

	sdk "github.com/cosmos/cosmos-sdk/types"
	ibcwasmtypes "github.com/cosmos/ibc-go/modules/light-clients/08-wasm/types"
	clienttypes "github.com/cosmos/ibc-go/v8/modules/core/02-client/types"
	channeltypes "github.com/cosmos/ibc-go/v8/modules/core/04-channel/types"
	v2 "github.com/cosmos/ibc-go/v8/modules/core/23-commitment/types/v2"
	ibcexported "github.com/cosmos/ibc-go/v8/modules/core/exported"
	ibctesting "github.com/cosmos/ibc-go/v8/testing"
	ethcommon "github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/ethclient"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/e2esuite"
	ethereumligthclient "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/ethereumlightclient"
	"github.com/stretchr/testify/suite"
)

const (
	ethRPC             = "http://localhost:32845"
	beaconRPC          = "http://localhost:32850"
	ics26RouterAddress = "0xC3536F63aB92bc7902dB5D57926c80f933121Bca"
)

type UnionTestSuite struct {
	e2esuite.TestSuite

	// The (hex encoded) checksum of the wasm client contract deployed on the Cosmos chain
	simdClientChecksum string
	simdClientID       string
	ethClientID        string

	clientState    ethereumligthclient.ClientState
	consensusState ethereumligthclient.ConsensusState
	tmpStorageRoot string
	blockNumberHex string
}

func (s *UnionTestSuite) SetupSuite(ctx context.Context) {
	s.TestSuite.SetupSuite(ctx)

	simd := s.ChainB
	_, simdRelayerUser := s.GetRelayerUsers(ctx)

	// Just to do the same as the other test suite:
	err := os.Chdir("../..")
	s.Require().NoError(err)

	file, err := os.Open("e2e/interchaintestv8/wasm/ethereum_light_client_mainnet.wasm.gz")
	s.Require().NoError(err)

	s.simdClientChecksum = s.PushNewWasmClientProposal(ctx, simd, simdRelayerUser, file)

	s.Require().NotEmpty(s.simdClientChecksum, "checksum was empty but should not have been")

	ethClient, err := ethclient.Dial(ethRPC)
	s.Require().NoError(err)
	var blockNumberHex string
	err = ethClient.Client().Call(&blockNumberHex, "eth_blockNumber")
	s.Require().NoError(err)
	s.blockNumberHex = blockNumberHex
	blockNumber, err := strconv.ParseInt(blockNumberHex, 0, 0)
	s.Require().NoError(err)

	time.Sleep(20 * time.Second) // Just to give time to settle, some calls might fail otherwise

	beaconAPIClient := NewBeaconAPIClient(beaconRPC)
	genesis := beaconAPIClient.GetGenesis()
	spec := beaconAPIClient.GetSpec()

	genesisValidatorsRoot := genesis.GenesisValidatorsRoot

	ethClientState := ethereumligthclient.ClientState{
		ChainId:                      "17000",
		GenesisValidatorsRoot:        genesisValidatorsRoot[:],
		MinSyncCommitteeParticipants: 0,
		GenesisTime:                  uint64(genesis.GenesisTime.Unix()),
		ForkParameters:               spec.ToForkParameters(),
		SecondsPerSlot:               uint64(spec.SecondsPerSlot),
		SlotsPerEpoch:                spec.SlotsPerEpoch,
		EpochsPerSyncCommitteePeriod: spec.EpochsPerSyncCommitteePeriod,
		LatestSlot:                   uint64(blockNumber),
		FrozenHeight: &clienttypes.Height{
			RevisionNumber: 0,
			RevisionHeight: 0,
		},
		IbcCommitmentSlot:  []byte{0, 0, 0, 0},                    // TODO: Does this change with a different contract in any way?
		IbcContractAddress: ethcommon.FromHex(ics26RouterAddress), // some random address
	}
	s.clientState = ethClientState

	fmt.Printf("client state: %+v\n", ethClientState)

	ethClientStateBz := simd.Config().EncodingConfig.Codec.MustMarshal(&ethClientState)
	wasmClientChecksum, err := hex.DecodeString(s.simdClientChecksum)
	s.Require().NoError(err)
	latestHeight := clienttypes.Height{
		RevisionNumber: 0, // TODO: 0 or 1?
		RevisionHeight: uint64(blockNumber),
	}
	clientState := ibcwasmtypes.ClientState{
		Data:         ethClientStateBz,
		Checksum:     wasmClientChecksum,
		LatestHeight: latestHeight,
	}
	clientStateAny, err := clienttypes.PackClientState(&clientState)
	s.Require().NoError(err)

	fmt.Printf(`
{
  "data": {
    "chain_id": "17000",
    "genesis_validators_root": "0x%s",
    "min_sync_committee_participants": 0,
    "genesis_time": %d,
    "fork_parameters": {
      "genesis_fork_version": "%s",
      "genesis_slot": %d,
      "altair": {
        "version": "0x%s",
        "epoch": %d
      },
      "bellatrix": {
        "version": "0x%s",
        "epoch": %d
      },
      "capella": {
        "version": "0x%s",
        "epoch": %d
      },
      "deneb": {
        "version": "0x%s",
        "epoch": %d
      }
    },
    "seconds_per_slot": %d,
    "slots_per_epoch": %d,
    "epochs_per_sync_committee_period": %d,
    "latest_slot": %d,
    "frozen_height": {
      "revision_number": 0,
      "revision_height": 0
    },
    "ibc_commitment_slot": "0",
    "ibc_contract_address": "%s"
  },
  "checksum": "%s",
  "latest_height": {
    "revision_number": %d,
    "revision_height": %d
  }
}\n`,
		ethcommon.Bytes2Hex(ethClientState.GenesisValidatorsRoot),
		ethClientState.GenesisTime,
		ethcommon.Bytes2Hex(spec.GenesisForkVersion[:]),
		spec.GenesisSlot,
		ethcommon.Bytes2Hex(spec.AltairForkVersion[:]),
		spec.AltairForkEpoch,
		ethcommon.Bytes2Hex(spec.BellatrixForkVersion[:]),
		spec.BellatrixForkEpoch,
		ethcommon.Bytes2Hex(spec.CapellaForkVersion[:]),
		spec.CapellaForkEpoch,
		ethcommon.Bytes2Hex(spec.DenebForkVersion[:]),
		spec.DenebForkEpoch,
		ethClientState.SecondsPerSlot,
		ethClientState.SlotsPerEpoch,
		ethClientState.EpochsPerSyncCommitteePeriod,
		ethClientState.LatestSlot,
		ics26RouterAddress,
		s.simdClientChecksum,
		latestHeight.RevisionNumber,
		latestHeight.RevisionHeight,
	)

	// fmt.Println()
	// fmt.Println("Client state")
	// fmt.Println("chain_id", ethClientState.ChainId)
	// fmt.Println("genesis_validators_root", ethcommon.Bytes2Hex(ethClientState.GenesisValidatorsRoot))
	// fmt.Println("min_sync_committee_participants: 0")
	// fmt.Println("genesis_time", ethClientState.GenesisTime)
	// fmt.Println("fork_parameters")
	// fmt.Println("genesis_fork_version", spec.GenesisForkVersion)
	// fmt.Println("genesis_slot", spec.GenesisSlot)
	// fmt.Println("altair")
	// fmt.Println("version", spec.AltairForkVersion)
	// fmt.Println("epoch", spec.AltairForkEpoch)
	// fmt.Println("bellatrix")
	// fmt.Println("version", spec.BellatrixForkVersion)
	// fmt.Println("epoch", spec.BellatrixForkEpoch)
	// fmt.Println("capella")
	// fmt.Println("version", spec.CapellaForkVersion)
	// fmt.Println("epoch", spec.CapellaForkEpoch)
	// fmt.Println("deneb")
	// fmt.Println("version", spec.DenebForkVersion)
	// fmt.Println("epoch", spec.DenebForkEpoch)
	// fmt.Println("seconds_per_slot", ethClientState.SecondsPerSlot)
	// fmt.Println("slots_per_epoch", ethClientState.SlotsPerEpoch)
	// fmt.Println("epochs_per_sync_committee_period", ethClientState.EpochsPerSyncCommitteePeriod)
	// fmt.Println("latest_slot", ethClientState.LatestSlot)
	// fmt.Println("ibc_contract_address", "0xe914d607a64c5ac3b2c9db3e1b5d809ec4c2e4bf")
	// fmt.Println("checksum", s.simdClientChecksum)
	// fmt.Println("latest_height")
	// fmt.Println("revision_number", clientState.LatestHeight.RevisionNumber)
	// fmt.Println("revision_height", clientState.LatestHeight.RevisionHeight)

	header := beaconAPIClient.GetHeader(strconv.Itoa(int(blockNumber)))
	bootstrap := beaconAPIClient.GetBootstrap(header.Root)
	timestamp := bootstrap.Data.Header.Execution.Timestamp * 1_000_000_000
	stateRoot := HexToBeBytes(bootstrap.Data.Header.Execution.StateRoot)

	fmt.Println("Client initial StateRoot:", bootstrap.Data.Header.Execution.StateRoot)

	proofResp := GetProof(ethClient, ics26RouterAddress, []string{}, blockNumberHex)

	currentPeriod := uint64(blockNumber) / spec.Period()
	clientUpdates := beaconAPIClient.GetLightClientUpdates(currentPeriod, 1)
	s.Require().Len(clientUpdates, 1)

	ethConsensusState := ethereumligthclient.ConsensusState{
		Slot:                 bootstrap.Data.Header.Beacon.Slot,
		StateRoot:            stateRoot,
		StorageRoot:          HexToBeBytes(proofResp.StorageHash),
		Timestamp:            timestamp,
		CurrentSyncCommittee: ethcommon.FromHex(bootstrap.Data.CurrentSyncCommittee.AggregatePubkey),
		NextSyncCommittee:    ethcommon.FromHex(clientUpdates[0].Data.NextSyncCommittee.AggregatePubkey),
	}
	s.consensusState = ethConsensusState

	fmt.Printf("Consensus state: %+v\n", ethConsensusState)

	fmt.Printf(`
{
  "data": {
    "slot": %d,
    "state_root": "%s",
    "storage_root": "%s",
    "timestamp": %d,
    "current_sync_committee": "%s",
    "next_sync_committee": "%s"
  }
}\n`,
		ethConsensusState.Slot,
		bootstrap.Data.Header.Execution.StateRoot,
		proofResp.StorageHash,
		timestamp,
		bootstrap.Data.CurrentSyncCommittee.AggregatePubkey,
		clientUpdates[0].Data.NextSyncCommittee.AggregatePubkey,
	)

	s.tmpStorageRoot = proofResp.StorageHash
	ethConsensusStateBz := simd.Config().EncodingConfig.Codec.MustMarshal(&ethConsensusState)
	consensusState := ibcwasmtypes.ConsensusState{
		Data: ethConsensusStateBz,
	}
	consensusStateAny, err := clienttypes.PackConsensusState(&consensusState)
	s.Require().NoError(err)

	res, err := s.BroadcastMessages(ctx, simd, simdRelayerUser, 200_000, &clienttypes.MsgCreateClient{
		ClientState:      clientStateAny,
		ConsensusState:   consensusStateAny,
		Signer:           simdRelayerUser.FormattedAddress(),
		CounterpartyId:   "",
		MerklePathPrefix: nil,
	})
	s.Require().NoError(err)

	s.simdClientID, err = ibctesting.ParseClientIDFromEvents(res.Events)
	s.Require().NoError(err)
	s.Require().Equal("08-wasm-0", s.simdClientID)
}

// TestWithIbcEurekaTestSuite is the boilerplate code that allows the test suite to be run
func TestWithUnionTestSuite(t *testing.T) {
	suite.Run(t, new(UnionTestSuite))
}

func (s *UnionTestSuite) TestUnionDeployment() {
	ctx := context.Background()

	s.SetupSuite(ctx)

	simd := s.ChainB
	ethClient, err := ethclient.Dial(ethRPC)
	s.Require().NoError(err)

	clientStateResp, err := e2esuite.GRPCQuery[clienttypes.QueryClientStateResponse](ctx, simd, &clienttypes.QueryClientStateRequest{
		ClientId: "08-wasm-0",
	})
	s.Require().NoError(err)

	clientStateAny := clientStateResp.ClientState

	var clientState ibcexported.ClientState
	err = simd.Config().EncodingConfig.InterfaceRegistry.UnpackAny(clientStateAny, &clientState)
	s.Require().NoError(err)

	wasmClientState, ok := clientState.(*ibcwasmtypes.ClientState)
	s.Require().True(ok)

	// Verify membership
	path := "commitments/ports/testport/channels/test-channel-0/sequences/1"
	storageKey := getStorageKey(path)
	storageKeys := []string{storageKey.Hex()}

	proofResp := GetProof(ethClient, ics26RouterAddress, storageKeys, s.blockNumberHex)
	s.Require().Len(proofResp.StorageProof, 1)

	var proofBz [][]byte
	for _, proofStr := range proofResp.StorageProof[0].Proof {
		proofBz = append(proofBz, ethcommon.FromHex(proofStr))
	}
	storageProof := ethereumligthclient.StorageProof{
		Key:   HexToBeBytes(proofResp.StorageProof[0].Key),
		Value: HexToBeBytes(proofResp.StorageProof[0].Value),
		Proof: proofBz,
	}
	storageProofBz := simd.Config().EncodingConfig.Codec.MustMarshal(&storageProof)

	fmt.Println()
	fmt.Println("Hex values for unit testing:")
	fmt.Printf("Key: %s\n", storageKey.Hex())
	fmt.Printf("Proof Key: %s\n", proofResp.StorageProof[0].Key)
	fmt.Printf("Proof Value: %s\n", proofResp.StorageProof[0].Value)
	fmt.Printf("Proof Proof: %+v\n", proofResp.StorageProof[0].Proof)
	fmt.Printf("Storage Root: %s\n", s.tmpStorageRoot)
	fmt.Printf("Storage Hash: %s\n", proofResp.StorageHash)

	// 	PathHash 04314c66dd5927303c5b1c010b29d3044d619ac94572d759d2c69e81da573842
	//
	// Hex values for unit testing:
	// Key: 0xab693306e3ccdded526ba13d80e7eadad21195ce64bc2a0c4a6e8dab1d236c69
	// Proof Key: 0xab693306e3ccdded526ba13d80e7eadad21195ce64bc2a0c4a6e8dab1d236c69
	// Proof Value: 0x6100b0115958fd2814360ce18f99b63e26266265f2a258a4191c11231c098c27
	// Proof Proof: [0xf871a044a87b9777d12d474a6c10c4d86751b4df9cdc995855f2e4e9b40f383aa58602808080a0200226d83fca4d6933b61c7a0
	// 18509ce6ad6e607ab1ce1077fc990995c48512480808080808080a01b56cc0a5b9b1ce34e9a14e896ea000c830bd64387573d238cbe3fa24ddfa2c3
	// 80808080 0xf871a033f5052fc658daefc52c34d7dfa38ce861b32252ac84397cebff9ee2f8e4d78f80a04b5917884cc6a74841ddf61f4d1f7373a1
	// 56028ba28be11927dba99e0f7e504480808080808080808080a0bc77e265cfb681fc5fc11ed6f1f24f6b99b3c0799535b32a5b65c4f582230441808
	// 080 0xf843a02008cce6a64280ab9a5a90acb4a00346c6ba86f71962b43328efd5c3fdceaabea1a06100b0115958fd2814360ce18f99b63e2626626
	// 5f2a258a4191c11231c098c27]
	// Storage Root: 0x10308a351689a587547d650cf46bfa13f0cf3423555310ab7ec048d8df5085a8
	// Storage Hash: 0x10308a351689a587547d650cf46bfa13f0cf3423555310ab7ec048d8df5085a8

	fmt.Println()
	merklePath := v2.MerklePath{
		KeyPath: [][]byte{[]byte(path)},
	}

	fmt.Println("Verify proof")
	fmt.Printf("Proof: %+v\n", storageProof)
	fmt.Printf("MerklePath: %+v\n", merklePath)

	packet := channeltypes.Packet{
		Sequence:           1,
		SourcePort:         "testport",
		SourceChannel:      "test-channel-0",
		DestinationPort:    "testport",
		DestinationChannel: "test-channel-1",
		Data:               []byte("testdata"),
		TimeoutHeight:      clienttypes.Height{},
		TimeoutTimestamp:   100 * 1_000_000_000,
	}
	value := rawValue(packet)
	fmt.Printf("Raw value hex: %s\n", ethcommon.Bytes2Hex(value))
	hashedValue := sha256.Sum256(value)
	fmt.Printf("Raw value hashed and hexed: %s\n", ethcommon.Bytes2Hex(hashedValue[:]))
	verifyResp, err := e2esuite.GRPCQuery[clienttypes.QueryVerifyMembershipResponse](ctx, simd, &clienttypes.QueryVerifyMembershipRequest{
		ClientId:    "08-wasm-0",
		Proof:       storageProofBz,
		ProofHeight: wasmClientState.LatestHeight,
		Value:       value,
		MerklePath:  merklePath,
	})

	// expected value (0xfce56304911af29f62fafbb8cc63e1284c58227678a2eef956e5ee5bd459de0e) and stored value (0x6100b0115958fd2814360ce18f99b63e26266265f2a258a4191c11231c098c27) don't match: wasm contract call failed"

	// :02AM ERR Verify membership failed KeyPath[0]=commitments/ports/testport/channels/test-channel-0/sequences/1 err="verify storage proof error: trie error (InvalidStateRoot(0x10308a351689a587547d650cf46bfa13f0cf3423555310ab7ec048d8df5085a8)) 0x10308a351689a587547d650cf46bfa13f0cf3423555310ab7ec048d8df5085a8 0xab693306e3ccdded526ba13d80e7eadad21195ce64bc2a0c4a6e8dab1d236c69 0x6100b0115958fd2814360ce18f99b63e26266265f2a258a4191c11231c098c27: trie error (InvalidStateRoot(0x10308a351689a587547d650cf46bfa13f0cf3423555310ab

	s.Require().NoError(err)
	fmt.Printf("Verify resp: %t\n", verifyResp.Success)
	fmt.Println(verifyResp.Success)
}

func TestRawValue(t *testing.T) {
	packet := channeltypes.Packet{
		Sequence:           1,
		SourcePort:         "testport",
		SourceChannel:      "test-channel-0",
		DestinationPort:    "testport",
		DestinationChannel: "test-channel-1",
		Data:               []byte("testdata"),
		TimeoutHeight:      clienttypes.Height{},
		TimeoutTimestamp:   100 * 1_000_000_000,
	}

	value := rawValue(packet)
	fmt.Printf("Raw value hex: %s\n", ethcommon.Bytes2Hex(value))
	hashedValue := sha256.Sum256(value)
	fmt.Printf("Raw value hashed and hexed: %s\n", ethcommon.Bytes2Hex(hashedValue[:]))
}

func HexToBeBytes(hex string) []byte {
	bz := ethcommon.FromHex(hex)
	if len(bz) == 32 {
		return bz
	}
	if len(bz) > 32 {
		panic("TOO BIG!")
	}
	beBytes := make([]byte, 32)
	copy(beBytes[32-len(bz):32], bz)
	return beBytes
}

func ToBeBytes(n *big.Int) [32]byte {
	bytes := n.Bytes()
	var beBytes [32]byte
	copy(beBytes[32-len(bytes):], bytes)
	return beBytes
}

func rawValue(packet channeltypes.Packet) []byte {
	timeoutHeight := packet.GetTimeoutHeight()

	buf := sdk.Uint64ToBigEndian(packet.GetTimeoutTimestamp())

	revisionNumber := sdk.Uint64ToBigEndian(timeoutHeight.GetRevisionNumber())
	buf = append(buf, revisionNumber...)

	revisionHeight := sdk.Uint64ToBigEndian(timeoutHeight.GetRevisionHeight())
	buf = append(buf, revisionHeight...)

	dataHash := sha256.Sum256(packet.GetData())
	buf = append(buf, dataHash[:]...)

	buf = append(buf, packet.GetDestPort()...)
	buf = append(buf, packet.GetDestChannel()...)

	return buf
}
