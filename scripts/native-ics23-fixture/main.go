package main

import (
	"bytes"
	"crypto/sha256"
	"encoding/binary"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"

	gogoproto "github.com/cosmos/gogoproto/proto"
	ics23 "github.com/cosmos/ics23/go"
	gogotypes "github.com/gogo/protobuf/types"
)

const (
	defaultMembershipSource    = "../../packages/tendermint-light-client/fixtures/verify_membership_key_0.json"
	defaultNonMembershipSource = "../../packages/tendermint-light-client/fixtures/verify_non-membership_key_1.json"
	defaultPacketSource        = "../../packages/tendermint-light-client/fixtures/verify_packet_commitment.json"
	defaultAckSource           = "../../packages/tendermint-light-client/fixtures/verify_acknowledgement_commitment.json"
	defaultReceiptSource       = "../../packages/tendermint-light-client/fixtures/verify_packet_receipt_absence.json"
	defaultMembershipOut       = "../../test/cometbft/fixtures/native_ics23_membership_fixture.json"
	defaultNonMembershipOut    = "../../test/cometbft/fixtures/native_ics23_non_membership_fixture.json"
	defaultRouterOut           = "../../test/cometbft/fixtures/native_ics23_router_fixture.json"
	castSig                    = "f(((uint8,(bytes,bytes,bool,(uint8,uint8,uint8,uint8,bytes),(uint8,bytes,bytes)[]),(bytes,(bool,(bytes,bytes,bool,(uint8,uint8,uint8,uint8,bytes),(uint8,bytes,bytes)[])),(bool,(bytes,bytes,bool,(uint8,uint8,uint8,uint8,bytes),(uint8,bytes,bytes)[]))))[]))"
)

type sourceFixture struct {
	ConsensusStateHex string `json:"consensus_state_hex"`
	MembershipMsg     struct {
		Height uint64   `json:"height"`
		Path   []string `json:"path"`
		Proof  string   `json:"proof"`
		Value  string   `json:"value"`
	} `json:"membership_msg"`
	Packet *sourcePacketContext `json:"packet,omitempty"`
}

type sourcePacketContext struct {
	Sequence         uint64                 `json:"sequence"`
	SourceClient     string                 `json:"source_client"`
	DestClient       string                 `json:"dest_client"`
	TimeoutTimestamp uint64                 `json:"timeout_timestamp"`
	Payloads         []sourcePayloadContext `json:"payloads"`
	Acknowledgement  []byte                 `json:"acknowledgement,omitempty"`
}

type sourcePayloadContext struct {
	SourcePort string `json:"source_port"`
	DestPort   string `json:"dest_port"`
	Version    string `json:"version"`
	Encoding   string `json:"encoding"`
	Value      []byte `json:"value"`
}

type nativeMembershipFixture struct {
	Source     string               `json:"source"`
	Membership nativeMembershipJSON `json:"membership"`
}

type nativeNonMembershipFixture struct {
	Source        string                  `json:"source"`
	NonMembership nativeNonMembershipJSON `json:"nonMembership"`
}

type nativeMembershipJSON struct {
	ProofHeight        uint64   `json:"proofHeight"`
	Path               []string `json:"path"`
	Value              string   `json:"value"`
	Root               string   `json:"root"`
	Timestamp          uint64   `json:"timestamp"`
	NextValidatorsHash string   `json:"nextValidatorsHash"`
	Proof              string   `json:"proof"`
}

type nativeNonMembershipJSON struct {
	ProofHeight        uint64   `json:"proofHeight"`
	Path               []string `json:"path"`
	Root               string   `json:"root"`
	Timestamp          uint64   `json:"timestamp"`
	NextValidatorsHash string   `json:"nextValidatorsHash"`
	Proof              string   `json:"proof"`
}

type nativeRouterFixture struct {
	ProofHeight               uint64           `json:"proofHeight"`
	Timestamp                 uint64           `json:"timestamp"`
	NextValidatorsHash        string           `json:"nextValidatorsHash"`
	Root                      string           `json:"root"`
	Packet                    routerPacketJSON `json:"packet"`
	LocalPacket               routerPacketJSON `json:"localPacket"`
	Acknowledgement           string           `json:"acknowledgement"`
	PacketCommitment          routerProofJSON  `json:"packetCommitment"`
	AcknowledgementCommitment routerProofJSON  `json:"acknowledgementCommitment"`
	PacketReceipt             routerProofJSON  `json:"packetReceipt"`
}

type routerPacketJSON struct {
	Sequence         uint64            `json:"sequence"`
	SourceClient     string            `json:"sourceClient"`
	DestClient       string            `json:"destClient"`
	TimeoutTimestamp uint64            `json:"timeoutTimestamp"`
	Payload          routerPayloadJSON `json:"payload"`
}

type routerPayloadJSON struct {
	SourcePort string `json:"sourcePort"`
	DestPort   string `json:"destPort"`
	Version    string `json:"version"`
	Encoding   string `json:"encoding"`
	Value      string `json:"value"`
}

type routerProofJSON struct {
	ProofHeight        uint64            `json:"proofHeight,omitempty"`
	Timestamp          uint64            `json:"timestamp,omitempty"`
	NextValidatorsHash string            `json:"nextValidatorsHash,omitempty"`
	Root               string            `json:"root,omitempty"`
	Packet             *routerPacketJSON `json:"packet,omitempty"`
	Path               []string          `json:"path"`
	Value              string            `json:"value,omitempty"`
	Proof              string            `json:"proof"`
}

type merkleProofProto struct {
	Proofs []*ics23.CommitmentProof `protobuf:"bytes,1,rep,name=proofs,proto3" json:"proofs,omitempty"`
}

func (m *merkleProofProto) Reset()         { *m = merkleProofProto{} }
func (m *merkleProofProto) String() string { return gogoproto.CompactTextString(m) }
func (*merkleProofProto) ProtoMessage()    {}

type merkleRootProto struct {
	Hash []byte `protobuf:"bytes,1,opt,name=hash,proto3" json:"hash,omitempty"`
}

func (m *merkleRootProto) Reset()         { *m = merkleRootProto{} }
func (m *merkleRootProto) String() string { return gogoproto.CompactTextString(m) }
func (*merkleRootProto) ProtoMessage()    {}

type consensusStateProto struct {
	Timestamp          *gogotypes.Timestamp `protobuf:"bytes,1,opt,name=timestamp,proto3" json:"timestamp,omitempty"`
	Root               *merkleRootProto     `protobuf:"bytes,2,opt,name=root,proto3" json:"root,omitempty"`
	NextValidatorsHash []byte               `protobuf:"bytes,3,opt,name=next_validators_hash,json=nextValidatorsHash,proto3" json:"next_validators_hash,omitempty"`
}

func (m *consensusStateProto) Reset()         { *m = consensusStateProto{} }
func (m *consensusStateProto) String() string { return gogoproto.CompactTextString(m) }
func (*consensusStateProto) ProtoMessage()    {}

func main() {
	if len(os.Args) < 2 {
		failf("usage: go run . membership [source] [out] | non-membership [source] [out]")
	}

	switch os.Args[1] {
	case "membership":
		source, out := paths(defaultMembershipSource, defaultMembershipOut)
		fix, err := buildNativeMembershipFixture(source)
		if err != nil {
			failf("%v", err)
		}
		writeJSON(out, fix)
	case "non-membership":
		source, out := paths(defaultNonMembershipSource, defaultNonMembershipOut)
		fix, err := buildNativeNonMembershipFixture(source)
		if err != nil {
			failf("%v", err)
		}
		writeJSON(out, fix)
	case "router":
		out := defaultRouterOut
		if len(os.Args) > 2 {
			out = os.Args[2]
		}
		fix, err := buildNativeRouterFixture()
		if err != nil {
			failf("%v", err)
		}
		writeJSON(out, fix)
	case "router-e2e":
		packetSource := defaultPacketSource
		ackSource := defaultAckSource
		receiptSource := defaultReceiptSource
		out := defaultRouterOut
		if len(os.Args) > 2 {
			packetSource = os.Args[2]
		}
		if len(os.Args) > 3 {
			ackSource = os.Args[3]
		}
		if len(os.Args) > 4 {
			receiptSource = os.Args[4]
		}
		if len(os.Args) > 5 {
			out = os.Args[5]
		}
		fix, err := buildNativeRouterFixtureFromSources(packetSource, ackSource, receiptSource)
		if err != nil {
			failf("%v", err)
		}
		writeJSON(out, fix)
	default:
		failf("unknown mode %q", os.Args[1])
	}
}

func paths(defaultSource string, defaultOut string) (string, string) {
	source := defaultSource
	out := defaultOut
	if len(os.Args) > 2 {
		source = os.Args[2]
	}
	if len(os.Args) > 3 {
		out = os.Args[3]
	}
	return source, out
}

func buildNativeMembershipFixture(sourcePath string) (nativeMembershipFixture, error) {
	source, consensusState, merkleProof, err := loadSource(sourcePath)
	if err != nil {
		return nativeMembershipFixture{}, err
	}
	if len(source.MembershipMsg.Path) != 2 {
		return nativeMembershipFixture{}, fmt.Errorf("expected two-segment membership path, got %d", len(source.MembershipMsg.Path))
	}
	if len(merkleProof.Proofs) != len(source.MembershipMsg.Path) {
		return nativeMembershipFixture{}, fmt.Errorf("proof/path length mismatch: %d proofs, %d path segments", len(merkleProof.Proofs), len(source.MembershipMsg.Path))
	}

	value, err := decodeHex(source.MembershipMsg.Value)
	if err != nil {
		return nativeMembershipFixture{}, fmt.Errorf("decode membership value: %w", err)
	}
	storeRoot, err := verifyRealMembershipProof(&source, &merkleProof, consensusState.Root.Hash, value)
	if err != nil {
		return nativeMembershipFixture{}, err
	}
	nativeProof, err := abiEncodeNativeMembershipProof(merkleProof.Proofs)
	if err != nil {
		return nativeMembershipFixture{}, err
	}
	if len(storeRoot) == 0 {
		return nativeMembershipFixture{}, fmt.Errorf("empty store root")
	}

	return nativeMembershipFixture{
		Source: filepath.Clean(sourcePath),
		Membership: nativeMembershipJSON{
			ProofHeight:        source.MembershipMsg.Height,
			Path:               hexPath(source.MembershipMsg.Path),
			Value:              hexBytes(value),
			Root:               hexBytes(consensusState.Root.Hash),
			Timestamp:          timestampNanos(consensusState.Timestamp),
			NextValidatorsHash: hexBytes(consensusState.NextValidatorsHash),
			Proof:              nativeProof,
		},
	}, nil
}

func buildNativeNonMembershipFixture(sourcePath string) (nativeNonMembershipFixture, error) {
	source, consensusState, merkleProof, err := loadSource(sourcePath)
	if err != nil {
		return nativeNonMembershipFixture{}, err
	}
	if len(source.MembershipMsg.Path) != 2 {
		return nativeNonMembershipFixture{}, fmt.Errorf("expected two-segment non-membership path, got %d", len(source.MembershipMsg.Path))
	}
	if len(merkleProof.Proofs) != len(source.MembershipMsg.Path) {
		return nativeNonMembershipFixture{}, fmt.Errorf("proof/path length mismatch: %d proofs, %d path segments", len(merkleProof.Proofs), len(source.MembershipMsg.Path))
	}

	storeRoot, err := verifyRealNonMembershipProof(&source, &merkleProof, consensusState.Root.Hash)
	if err != nil {
		return nativeNonMembershipFixture{}, err
	}
	nativeProof, err := abiEncodeNativeNonMembershipProof(merkleProof.Proofs)
	if err != nil {
		return nativeNonMembershipFixture{}, err
	}
	if len(storeRoot) == 0 {
		return nativeNonMembershipFixture{}, fmt.Errorf("empty store root")
	}

	return nativeNonMembershipFixture{
		Source: filepath.Clean(sourcePath),
		NonMembership: nativeNonMembershipJSON{
			ProofHeight:        source.MembershipMsg.Height,
			Path:               hexPath(source.MembershipMsg.Path),
			Root:               hexBytes(consensusState.Root.Hash),
			Timestamp:          timestampNanos(consensusState.Timestamp),
			NextValidatorsHash: hexBytes(consensusState.NextValidatorsHash),
			Proof:              nativeProof,
		},
	}, nil
}

func buildNativeRouterFixture() (nativeRouterFixture, error) {
	const (
		sourceClient       = "client-0"
		counterpartyClient = "client-1"
		port               = "transfer"
		version            = "ics20-1"
		encoding           = "json"
		sequence           = uint64(1)
		timeoutTimestamp   = uint64(2_000_000_000)
		proofHeight        = uint64(37)
		consensusTimestamp = uint64(2_000_000_000_000_000_000)
	)

	payload := []byte{0x01, 0x02, 0x03}
	acknowledgement := []byte("ack")
	ibcKey := []byte("ibc")
	packetCommitmentKey := packetPath(counterpartyClient, 1, sequence)
	packetReceiptKey := packetPath(counterpartyClient, 2, sequence)
	packetAckKey := packetPath(counterpartyClient, 3, sequence)
	packetCommitmentValue := packetCommitment(sourceClient, port, version, encoding, payload, timeoutTimestamp)
	ackValue := acknowledgementCommitment(acknowledgement)

	packetLeaf := routerLeafExistenceProof(packetCommitmentKey, packetCommitmentValue)
	ackLeaf := routerLeafExistenceProof(packetAckKey, ackValue)
	leftRoot, err := packetLeaf.Calculate()
	if err != nil {
		return nativeRouterFixture{}, fmt.Errorf("calculate packet commitment leaf: %w", err)
	}
	rightRoot, err := ackLeaf.Calculate()
	if err != nil {
		return nativeRouterFixture{}, fmt.Errorf("calculate ack leaf: %w", err)
	}

	packetProof := routerExistenceProof(packetCommitmentKey, packetCommitmentValue, nil, append([]byte{0x20}, rightRoot...))
	ackProof := routerExistenceProof(packetAckKey, ackValue, append(append([]byte{0x02, 0x00, 0x00, 0x20}, leftRoot...), 0x20), nil)
	storeRoot, err := packetProof.GetExist().Calculate()
	if err != nil {
		return nativeRouterFixture{}, fmt.Errorf("calculate packet commitment root with sibling: %w", err)
	}
	ackRoot, err := ackProof.GetExist().Calculate()
	if err != nil {
		return nativeRouterFixture{}, fmt.Errorf("calculate ack root with sibling: %w", err)
	}
	if !bytes.Equal(storeRoot, ackRoot) {
		return nativeRouterFixture{}, fmt.Errorf("synthetic router IAVL roots differ")
	}

	parentProof := routerParentProof(ibcKey, storeRoot)
	appRoot, err := parentProof.GetExist().Calculate()
	if err != nil {
		return nativeRouterFixture{}, fmt.Errorf("calculate app root: %w", err)
	}
	nonProof := &ics23.CommitmentProof{
		Proof: &ics23.CommitmentProof_Nonexist{
			Nonexist: &ics23.NonExistenceProof{
				Key:   packetReceiptKey,
				Left:  packetProof.GetExist(),
				Right: ackProof.GetExist(),
			},
		},
	}

	if !ics23.VerifyMembership(ics23.IavlSpec, storeRoot, packetProof, packetCommitmentKey, packetCommitmentValue) {
		return nativeRouterFixture{}, fmt.Errorf("packet commitment reference verification failed")
	}
	if !ics23.VerifyMembership(ics23.IavlSpec, storeRoot, ackProof, packetAckKey, ackValue) {
		return nativeRouterFixture{}, fmt.Errorf("acknowledgement reference verification failed")
	}
	if !ics23.VerifyNonMembership(ics23.IavlSpec, storeRoot, nonProof, packetReceiptKey) {
		return nativeRouterFixture{}, fmt.Errorf("packet receipt absence reference verification failed")
	}
	if !ics23.VerifyMembership(ics23.TendermintSpec, appRoot, parentProof, ibcKey, storeRoot) {
		return nativeRouterFixture{}, fmt.Errorf("router parent proof reference verification failed")
	}

	packetMembershipProof, err := abiEncodeNativeMembershipProof([]*ics23.CommitmentProof{packetProof, parentProof})
	if err != nil {
		return nativeRouterFixture{}, err
	}
	ackMembershipProof, err := abiEncodeNativeMembershipProof([]*ics23.CommitmentProof{ackProof, parentProof})
	if err != nil {
		return nativeRouterFixture{}, err
	}
	receiptNonMembershipProof, err := abiEncodeNativeNonMembershipProof([]*ics23.CommitmentProof{nonProof, parentProof})
	if err != nil {
		return nativeRouterFixture{}, err
	}

	payloadJSON := routerPayloadJSON{
		SourcePort: port,
		DestPort:   port,
		Version:    version,
		Encoding:   encoding,
		Value:      hexBytes(payload),
	}
	return nativeRouterFixture{
		ProofHeight:        proofHeight,
		Timestamp:          consensusTimestamp,
		NextValidatorsHash: hexBytes(make([]byte, 32)),
		Root:               hexBytes(appRoot),
		Packet: routerPacketJSON{
			Sequence:         sequence,
			SourceClient:     counterpartyClient,
			DestClient:       sourceClient,
			TimeoutTimestamp: timeoutTimestamp,
			Payload:          payloadJSON,
		},
		LocalPacket: routerPacketJSON{
			Sequence:         sequence,
			SourceClient:     sourceClient,
			DestClient:       counterpartyClient,
			TimeoutTimestamp: timeoutTimestamp,
			Payload:          payloadJSON,
		},
		Acknowledgement: hexBytes(acknowledgement),
		PacketCommitment: routerProofJSON{
			ProofHeight:        proofHeight,
			Timestamp:          consensusTimestamp,
			NextValidatorsHash: hexBytes(make([]byte, 32)),
			Root:               hexBytes(appRoot),
			Packet: &routerPacketJSON{
				Sequence:         sequence,
				SourceClient:     counterpartyClient,
				DestClient:       sourceClient,
				TimeoutTimestamp: timeoutTimestamp,
				Payload:          payloadJSON,
			},
			Path:  hexPathBytes([][]byte{ibcKey, packetCommitmentKey}),
			Value: hexBytes(packetCommitmentValue),
			Proof: packetMembershipProof,
		},
		AcknowledgementCommitment: routerProofJSON{
			ProofHeight:        proofHeight,
			Timestamp:          consensusTimestamp,
			NextValidatorsHash: hexBytes(make([]byte, 32)),
			Root:               hexBytes(appRoot),
			Packet: &routerPacketJSON{
				Sequence:         sequence,
				SourceClient:     sourceClient,
				DestClient:       counterpartyClient,
				TimeoutTimestamp: timeoutTimestamp,
				Payload:          payloadJSON,
			},
			Path:  hexPathBytes([][]byte{ibcKey, packetAckKey}),
			Value: hexBytes(ackValue),
			Proof: ackMembershipProof,
		},
		PacketReceipt: routerProofJSON{
			ProofHeight:        proofHeight,
			Timestamp:          consensusTimestamp,
			NextValidatorsHash: hexBytes(make([]byte, 32)),
			Root:               hexBytes(appRoot),
			Packet: &routerPacketJSON{
				Sequence:         sequence,
				SourceClient:     sourceClient,
				DestClient:       counterpartyClient,
				TimeoutTimestamp: timeoutTimestamp,
				Payload:          payloadJSON,
			},
			Path:  hexPathBytes([][]byte{ibcKey, packetReceiptKey}),
			Proof: receiptNonMembershipProof,
		},
	}, nil
}

func buildNativeRouterFixtureFromSources(
	packetSource string,
	ackSource string,
	receiptSource string,
) (nativeRouterFixture, error) {
	packetMembership, err := buildNativeMembershipFixture(packetSource)
	if err != nil {
		return nativeRouterFixture{}, fmt.Errorf("packet commitment fixture: %w", err)
	}
	ackMembership, err := buildNativeMembershipFixture(ackSource)
	if err != nil {
		return nativeRouterFixture{}, fmt.Errorf("acknowledgement commitment fixture: %w", err)
	}
	receiptNonMembership, err := buildNativeNonMembershipFixture(receiptSource)
	if err != nil {
		return nativeRouterFixture{}, fmt.Errorf("packet receipt fixture: %w", err)
	}

	packetSrc, _, _, err := loadSource(packetSource)
	if err != nil {
		return nativeRouterFixture{}, err
	}
	ackSrc, _, _, err := loadSource(ackSource)
	if err != nil {
		return nativeRouterFixture{}, err
	}
	receiptSrc, _, _, err := loadSource(receiptSource)
	if err != nil {
		return nativeRouterFixture{}, err
	}

	counterpartyPacket, err := routerPacketFromSource(packetSrc.Packet)
	if err != nil {
		return nativeRouterFixture{}, fmt.Errorf("packet commitment metadata: %w", err)
	}
	ackPacket, err := routerPacketFromSource(ackSrc.Packet)
	if err != nil {
		return nativeRouterFixture{}, fmt.Errorf("acknowledgement metadata: %w", err)
	}
	timeoutPacket, err := routerPacketFromSource(receiptSrc.Packet)
	if err != nil {
		return nativeRouterFixture{}, fmt.Errorf("packet receipt metadata: %w", err)
	}
	if ackSrc.Packet == nil || len(ackSrc.Packet.Acknowledgement) == 0 {
		return nativeRouterFixture{}, fmt.Errorf("acknowledgement fixture missing acknowledgement bytes")
	}

	packetProof := packetMembership.Membership
	ackProof := ackMembership.Membership
	receiptProof := receiptNonMembership.NonMembership
	return nativeRouterFixture{
		ProofHeight:        packetProof.ProofHeight,
		Timestamp:          packetProof.Timestamp,
		NextValidatorsHash: packetProof.NextValidatorsHash,
		Root:               packetProof.Root,
		Packet:             counterpartyPacket,
		LocalPacket:        ackPacket,
		Acknowledgement:    hexBytes(ackSrc.Packet.Acknowledgement),
		PacketCommitment: routerProofJSON{
			ProofHeight:        packetProof.ProofHeight,
			Timestamp:          packetProof.Timestamp,
			NextValidatorsHash: packetProof.NextValidatorsHash,
			Root:               packetProof.Root,
			Packet:             &counterpartyPacket,
			Path:               packetProof.Path,
			Value:              packetProof.Value,
			Proof:              packetProof.Proof,
		},
		AcknowledgementCommitment: routerProofJSON{
			ProofHeight:        ackProof.ProofHeight,
			Timestamp:          ackProof.Timestamp,
			NextValidatorsHash: ackProof.NextValidatorsHash,
			Root:               ackProof.Root,
			Packet:             &ackPacket,
			Path:               ackProof.Path,
			Value:              ackProof.Value,
			Proof:              ackProof.Proof,
		},
		PacketReceipt: routerProofJSON{
			ProofHeight:        receiptProof.ProofHeight,
			Timestamp:          receiptProof.Timestamp,
			NextValidatorsHash: receiptProof.NextValidatorsHash,
			Root:               receiptProof.Root,
			Packet:             &timeoutPacket,
			Path:               receiptProof.Path,
			Proof:              receiptProof.Proof,
		},
	}, nil
}

func loadSource(sourcePath string) (sourceFixture, consensusStateProto, merkleProofProto, error) {
	sourceBz, err := os.ReadFile(sourcePath)
	if err != nil {
		return sourceFixture{}, consensusStateProto{}, merkleProofProto{}, err
	}
	var source sourceFixture
	if err := json.Unmarshal(sourceBz, &source); err != nil {
		return sourceFixture{}, consensusStateProto{}, merkleProofProto{}, err
	}

	consensusStateBz, err := decodeHex(source.ConsensusStateHex)
	if err != nil {
		return sourceFixture{}, consensusStateProto{}, merkleProofProto{}, fmt.Errorf("decode consensus_state_hex: %w", err)
	}
	var consensusState consensusStateProto
	if err := gogoproto.Unmarshal(consensusStateBz, &consensusState); err != nil {
		return sourceFixture{}, consensusStateProto{}, merkleProofProto{}, fmt.Errorf("decode consensus state: %w", err)
	}
	if consensusState.Timestamp == nil || consensusState.Root == nil || len(consensusState.Root.Hash) != 32 {
		return sourceFixture{}, consensusStateProto{}, merkleProofProto{}, fmt.Errorf("consensus state missing timestamp or 32-byte root")
	}

	proofBz, err := decodeHex(source.MembershipMsg.Proof)
	if err != nil {
		return sourceFixture{}, consensusStateProto{}, merkleProofProto{}, fmt.Errorf("decode proof: %w", err)
	}
	var merkleProof merkleProofProto
	if err := gogoproto.Unmarshal(proofBz, &merkleProof); err != nil {
		return sourceFixture{}, consensusStateProto{}, merkleProofProto{}, fmt.Errorf("decode merkle proof: %w", err)
	}
	return source, consensusState, merkleProof, nil
}

func verifyRealMembershipProof(
	source *sourceFixture,
	merkleProof *merkleProofProto,
	appHash []byte,
	value []byte,
) ([]byte, error) {
	leafProof := merkleProof.Proofs[0]
	storeProof := merkleProof.Proofs[1]
	if leafProof.GetExist() == nil || storeProof.GetExist() == nil {
		return nil, fmt.Errorf("membership fixture must contain existence proofs")
	}

	leafKey := []byte(source.MembershipMsg.Path[1])
	storeKey := []byte(source.MembershipMsg.Path[0])
	storeRoot, err := leafProof.GetExist().Calculate()
	if err != nil {
		return nil, fmt.Errorf("calculate store root: %w", err)
	}
	if !ics23.VerifyMembership(ics23.IavlSpec, storeRoot, leafProof, leafKey, value) {
		return nil, fmt.Errorf("IAVL membership proof failed reference verification")
	}
	if !ics23.VerifyMembership(ics23.TendermintSpec, appHash, storeProof, storeKey, storeRoot) {
		return nil, fmt.Errorf("Tendermint membership proof failed reference verification")
	}
	return storeRoot, nil
}

func verifyRealNonMembershipProof(
	source *sourceFixture,
	merkleProof *merkleProofProto,
	appHash []byte,
) ([]byte, error) {
	nonProof := merkleProof.Proofs[0]
	storeProof := merkleProof.Proofs[1]
	nonExistence := nonProof.GetNonexist()
	if nonExistence == nil || storeProof.GetExist() == nil {
		return nil, fmt.Errorf("non-membership fixture must contain non-existence then existence proofs")
	}

	storeRoot, err := nonExistenceRoot(nonExistence)
	if err != nil {
		return nil, err
	}
	leafKey := []byte(source.MembershipMsg.Path[1])
	storeKey := []byte(source.MembershipMsg.Path[0])
	if !ics23.VerifyNonMembership(ics23.IavlSpec, storeRoot, nonProof, leafKey) {
		return nil, fmt.Errorf("IAVL non-membership proof failed reference verification")
	}
	if !ics23.VerifyMembership(ics23.TendermintSpec, appHash, storeProof, storeKey, storeRoot) {
		return nil, fmt.Errorf("Tendermint membership proof failed reference verification")
	}
	return storeRoot, nil
}

func nonExistenceRoot(proof *ics23.NonExistenceProof) ([]byte, error) {
	var root []byte
	if proof.Left != nil {
		leftRoot, err := proof.Left.Calculate()
		if err != nil {
			return nil, fmt.Errorf("calculate left neighbor root: %w", err)
		}
		root = leftRoot
	}
	if proof.Right != nil {
		rightRoot, err := proof.Right.Calculate()
		if err != nil {
			return nil, fmt.Errorf("calculate right neighbor root: %w", err)
		}
		if root != nil && !bytes.Equal(root, rightRoot) {
			return nil, fmt.Errorf("non-membership neighbor roots differ")
		}
		root = rightRoot
	}
	if root == nil {
		return nil, fmt.Errorf("non-membership proof has no neighbors")
	}
	return root, nil
}

func routerPacketFromSource(packet *sourcePacketContext) (routerPacketJSON, error) {
	if packet == nil {
		return routerPacketJSON{}, fmt.Errorf("missing packet metadata")
	}
	if len(packet.Payloads) != 1 {
		return routerPacketJSON{}, fmt.Errorf("expected one payload, got %d", len(packet.Payloads))
	}
	payload := packet.Payloads[0]
	return routerPacketJSON{
		Sequence:         packet.Sequence,
		SourceClient:     packet.SourceClient,
		DestClient:       packet.DestClient,
		TimeoutTimestamp: packet.TimeoutTimestamp,
		Payload: routerPayloadJSON{
			SourcePort: payload.SourcePort,
			DestPort:   payload.DestPort,
			Version:    payload.Version,
			Encoding:   payload.Encoding,
			Value:      hexBytes(payload.Value),
		},
	}, nil
}

func abiEncodeNativeMembershipProof(proofs []*ics23.CommitmentProof) (string, error) {
	parts := make([]string, len(proofs))
	for i, proof := range proofs {
		existence := proof.GetExist()
		if existence == nil {
			return "", fmt.Errorf("proof %d is not an existence proof", i)
		}
		parts[i] = fmt.Sprintf("(1,%s,%s)", existenceTuple(existence), emptyNonExistenceTuple())
	}
	return castABIEncode("([" + strings.Join(parts, ",") + "])")
}

func abiEncodeNativeNonMembershipProof(proofs []*ics23.CommitmentProof) (string, error) {
	if len(proofs) != 2 || proofs[0].GetNonexist() == nil || proofs[1].GetExist() == nil {
		return "", fmt.Errorf("expected non-existence proof followed by existence proof")
	}
	parts := []string{
		fmt.Sprintf("(2,%s,%s)", emptyExistenceTuple(), nonExistenceTuple(proofs[0].GetNonexist())),
		fmt.Sprintf("(1,%s,%s)", existenceTuple(proofs[1].GetExist()), emptyNonExistenceTuple()),
	}
	return castABIEncode("([" + strings.Join(parts, ",") + "])")
}

func castABIEncode(arg string) (string, error) {
	cmd := exec.Command("cast", "abi-encode", castSig, arg)
	out, err := cmd.Output()
	if err != nil {
		if exitErr, ok := err.(*exec.ExitError); ok {
			return "", fmt.Errorf("cast abi-encode failed: %s", string(exitErr.Stderr))
		}
		return "", err
	}
	return strings.TrimSpace(string(out)), nil
}

func existenceTuple(proof *ics23.ExistenceProof) string {
	leaf := proof.GetLeaf()
	hasLeaf := leaf != nil
	leafTuple := "(0,0,0,0,0x)"
	if leaf != nil {
		leafTuple = fmt.Sprintf("(%d,%d,%d,%d,%s)", leaf.Hash, leaf.PrehashKey, leaf.PrehashValue, leaf.Length, hexBytes(leaf.Prefix))
	}
	innerOps := make([]string, len(proof.Path))
	for i, op := range proof.Path {
		innerOps[i] = fmt.Sprintf("(%d,%s,%s)", op.Hash, hexBytes(op.Prefix), hexBytes(op.Suffix))
	}
	return fmt.Sprintf("(%s,%s,%t,%s,[%s])", hexBytes(proof.Key), hexBytes(proof.Value), hasLeaf, leafTuple, strings.Join(innerOps, ","))
}

func nonExistenceTuple(proof *ics23.NonExistenceProof) string {
	return fmt.Sprintf(
		"(%s,%s,%s)",
		hexBytes(proof.Key),
		optionalExistenceTuple(proof.Left),
		optionalExistenceTuple(proof.Right),
	)
}

func optionalExistenceTuple(proof *ics23.ExistenceProof) string {
	if proof == nil {
		return emptyOptionalExistenceTuple()
	}
	return fmt.Sprintf("(true,%s)", existenceTuple(proof))
}

func emptyExistenceTuple() string {
	return "(0x,0x,false,(0,0,0,0,0x),[])"
}

func emptyOptionalExistenceTuple() string {
	return fmt.Sprintf("(false,%s)", emptyExistenceTuple())
}

func emptyNonExistenceTuple() string {
	return fmt.Sprintf("(0x,%s,%s)", emptyOptionalExistenceTuple(), emptyOptionalExistenceTuple())
}

func routerExistenceProof(key []byte, value []byte, prefix []byte, suffix []byte) *ics23.CommitmentProof {
	if prefix == nil {
		prefix = []byte{0x02, 0x00, 0x00, 0x20}
	}
	existence := routerLeafExistenceProof(key, value)
	existence.Path = []*ics23.InnerOp{{
		Hash:   ics23.HashOp_SHA256,
		Prefix: prefix,
		Suffix: suffix,
	}}
	return &ics23.CommitmentProof{
		Proof: &ics23.CommitmentProof_Exist{
			Exist: existence,
		},
	}
}

func routerLeafExistenceProof(key []byte, value []byte) *ics23.ExistenceProof {
	return &ics23.ExistenceProof{
		Key:   key,
		Value: value,
		Leaf: &ics23.LeafOp{
			Hash:         ics23.HashOp_SHA256,
			PrehashKey:   ics23.HashOp_NO_HASH,
			PrehashValue: ics23.HashOp_SHA256,
			Length:       ics23.LengthOp_VAR_PROTO,
			Prefix:       []byte{0x00, 0x00, 0x00},
		},
	}
}

func routerParentProof(key []byte, value []byte) *ics23.CommitmentProof {
	return &ics23.CommitmentProof{
		Proof: &ics23.CommitmentProof_Exist{
			Exist: &ics23.ExistenceProof{
				Key:   key,
				Value: value,
				Leaf: &ics23.LeafOp{
					Hash:         ics23.HashOp_SHA256,
					PrehashKey:   ics23.HashOp_NO_HASH,
					PrehashValue: ics23.HashOp_SHA256,
					Length:       ics23.LengthOp_VAR_PROTO,
					Prefix:       []byte{0x00},
				},
				Path: []*ics23.InnerOp{{
					Hash:   ics23.HashOp_SHA256,
					Prefix: []byte{0x01},
				}},
			},
		},
	}
}

func packetPath(clientID string, kind byte, sequence uint64) []byte {
	out := append([]byte(clientID), kind)
	var sequenceBz [8]byte
	binary.BigEndian.PutUint64(sequenceBz[:], sequence)
	return append(out, sequenceBz[:]...)
}

func packetCommitment(
	destClient string,
	port string,
	version string,
	encoding string,
	payload []byte,
	timeoutTimestamp uint64,
) []byte {
	appHash := hashPayload(port, version, encoding, payload)
	var timeoutBz [8]byte
	binary.BigEndian.PutUint64(timeoutBz[:], timeoutTimestamp)
	return sha256Bytes(bytes.Join([][]byte{
		{0x02},
		sha256Bytes([]byte(destClient)),
		sha256Bytes(timeoutBz[:]),
		sha256Bytes(appHash),
	}, nil))
}

func hashPayload(port string, version string, encoding string, payload []byte) []byte {
	return sha256Bytes(bytes.Join([][]byte{
		sha256Bytes([]byte(port)),
		sha256Bytes([]byte(port)),
		sha256Bytes([]byte(version)),
		sha256Bytes([]byte(encoding)),
		sha256Bytes(payload),
	}, nil))
}

func acknowledgementCommitment(ack []byte) []byte {
	return sha256Bytes(append([]byte{0x02}, sha256Bytes(ack)...))
}

func sha256Bytes(bz []byte) []byte {
	sum := sha256.Sum256(bz)
	return sum[:]
}

func writeJSON(out string, value any) {
	bz, err := json.MarshalIndent(value, "", "  ")
	if err != nil {
		failf("%v", err)
	}
	bz = append(bz, '\n')
	if err := os.MkdirAll(filepath.Dir(out), 0o755); err != nil {
		failf("%v", err)
	}
	if err := os.WriteFile(out, bz, 0o644); err != nil {
		failf("%v", err)
	}
	fmt.Println(out)
}

func timestampNanos(t *gogotypes.Timestamp) uint64 {
	return uint64(t.Seconds)*1_000_000_000 + uint64(t.Nanos)
}

func hexBytes(bz []byte) string {
	return "0x" + hex.EncodeToString(bz)
}

func hexPath(path []string) []string {
	out := make([]string, len(path))
	for i, segment := range path {
		out[i] = hexBytes([]byte(segment))
	}
	return out
}

func hexPathBytes(path [][]byte) []string {
	out := make([]string, len(path))
	for i, segment := range path {
		out[i] = hexBytes(segment)
	}
	return out
}

func decodeHex(value string) ([]byte, error) {
	return hex.DecodeString(strings.TrimPrefix(value, "0x"))
}

func failf(format string, args ...any) {
	fmt.Fprintf(os.Stderr, format+"\n", args...)
	os.Exit(1)
}
