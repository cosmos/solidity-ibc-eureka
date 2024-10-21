package main

import (
	"context"
	"crypto/sha256"
	"fmt"
	"testing"

	"github.com/stretchr/testify/suite"

	ethcommon "github.com/ethereum/go-ethereum/common"

	sdk "github.com/cosmos/cosmos-sdk/types"

	ibcwasmtypes "github.com/cosmos/ibc-go/modules/light-clients/08-wasm/types"
	clienttypes "github.com/cosmos/ibc-go/v8/modules/core/02-client/types"
	channeltypes "github.com/cosmos/ibc-go/v8/modules/core/04-channel/types"
	v2 "github.com/cosmos/ibc-go/v8/modules/core/23-commitment/types/v2"
	ibcexported "github.com/cosmos/ibc-go/v8/modules/core/exported"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/e2esuite"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/ethereum"
	ethereumligthclient "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/ethereumlightclient"
)

const (
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

// TestWithIbcEurekaTestSuite is the boilerplate code that allows the test suite to be run
func TestWithUnionTestSuite(t *testing.T) {
	suite.Run(t, new(UnionTestSuite))
}

func (s *UnionTestSuite) TestDeploy() {
	ctx := context.Background()

	s.SetupSuite(ctx)

	s.Require().True(true)
}

func (s *UnionTestSuite) TestUnionDeployment() {
	ctx := context.Background()

	s.SetupSuite(ctx)

	eth, simd := s.ChainA, s.ChainB

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
	storageKey := ethereum.GetStorageKey(path)
	storageKeys := []string{storageKey.Hex()}

	proofResp, err := eth.EthAPI.GetProof(ics26RouterAddress, storageKeys, s.blockNumberHex)
	s.Require().NoError(err)
	s.Require().Len(proofResp.StorageProof, 1)

	var proofBz [][]byte
	for _, proofStr := range proofResp.StorageProof[0].Proof {
		proofBz = append(proofBz, ethcommon.FromHex(proofStr))
	}
	storageProof := ethereumligthclient.StorageProof{
		Key:   ethereum.HexToBeBytes(proofResp.StorageProof[0].Key),
		Value: ethereum.HexToBeBytes(proofResp.StorageProof[0].Value),
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

	s.Require().NoError(err)
	fmt.Printf("Verify resp: %t\n", verifyResp.Success)
	s.Require().True(verifyResp.Success)
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
