package solana

import (
	"context"
	"crypto/sha256"
	"encoding/binary"
	"fmt"

	"github.com/gagliardetto/solana-go"
	"github.com/gagliardetto/solana-go/rpc"

	access_manager "github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/accessmanager"
	ics07_tendermint "github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/ics07tendermint"
	ics26_router "github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/ics26router"
	ics27_gmp "github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/ics27gmp"
)

func (s *Solana) GetNextSequenceNumber(ctx context.Context, clientSequencePDA solana.PublicKey) (uint64, error) {
	// Use confirmed commitment to match relayer read commitment level
	clientSequenceAccount, err := s.RPCClient.GetAccountInfoWithOpts(ctx, clientSequencePDA, &rpc.GetAccountInfoOpts{
		Commitment: rpc.CommitmentConfirmed,
	})
	if err != nil || clientSequenceAccount.Value == nil {
		return 1, nil
	}

	data := clientSequenceAccount.Value.Data.GetBinary()
	if len(data) < 17 {
		return 0, fmt.Errorf("client sequence account data too short: expected >= 17 bytes, got %d", len(data))
	}

	nextSequence := binary.LittleEndian.Uint64(data[9:17])
	return nextSequence, nil
}

// CalculateNamespacedSequence mirrors the on-chain Rust logic for sequence namespacing.
// Formula: sequence = base_sequence * 10000 + suffix
// where suffix = hash(calling_program || sender) % 10000
func CalculateNamespacedSequence(baseSequence uint64, callingProgram, sender solana.PublicKey) uint64 {
	hasher := sha256.New()
	hasher.Write(callingProgram.Bytes())
	hasher.Write(sender.Bytes())
	hash := hasher.Sum(nil)

	rawU16 := binary.LittleEndian.Uint16(hash[0:2])
	suffix := uint64(rawU16 % 10000)

	return baseSequence*10000 + suffix
}

// GMPCallResultPDA derives the PDA for a GMP call result account.
// This PDA stores the acknowledgement or timeout result of a GMP call.
// Seeds: ["gmp_result", source_client, sequence (little-endian u64)]
func GMPCallResultPDA(programID solana.PublicKey, sourceClient string, sequence uint64) (solana.PublicKey, uint8) {
	seqBytes := make([]byte, 8)
	binary.LittleEndian.PutUint64(seqBytes, sequence)
	pda, bump, err := solana.FindProgramAddress(
		[][]byte{[]byte("gmp_result"), []byte(sourceClient), seqBytes},
		programID,
	)
	if err != nil {
		panic(fmt.Sprintf("failed to derive GMPCallResultPDA: %v", err))
	}
	return pda, bump
}

// CallResultStatus represents the status of a GMP call result.
type CallResultStatus uint8

const (
	// CallResultStatusAcknowledgement indicates the call received an acknowledgement.
	CallResultStatusAcknowledgement CallResultStatus = 0
	// CallResultStatusTimeout indicates the call timed out.
	CallResultStatusTimeout CallResultStatus = 1
)

// GMPCallResultAccount represents the on-chain data for a GMP call result.
type GMPCallResultAccount struct {
	Version         uint8            // Account schema version (0 = V1)
	Sender          string           // Original sender address on the source chain
	Sequence        uint64           // IBC packet sequence number
	SourceClient    string           // Source client ID
	DestClient      string           // Destination client ID
	Status          CallResultStatus // Acknowledgement or Timeout
	Acknowledgement []byte           // Acknowledgement data (empty for timeout)
	ResultTimestamp int64            // Timestamp when result was recorded (Unix seconds)
	Bump            uint8            // PDA bump seed
}

// DecodeGMPCallResultAccount deserializes a GMPCallResultAccount from Borsh-encoded data.
// The data should include the 8-byte Anchor discriminator prefix.
func DecodeGMPCallResultAccount(data []byte) (*GMPCallResultAccount, error) {
	if len(data) < 8 {
		return nil, fmt.Errorf("data too short: need at least 8 bytes for discriminator, got %d", len(data))
	}

	// Skip 8-byte discriminator
	offset := 8

	// Helper to read u32 length-prefixed string
	readString := func() (string, error) {
		if offset+4 > len(data) {
			return "", fmt.Errorf("not enough data for string length at offset %d", offset)
		}
		strLen := binary.LittleEndian.Uint32(data[offset:])
		offset += 4
		if offset+int(strLen) > len(data) {
			return "", fmt.Errorf("not enough data for string of length %d at offset %d", strLen, offset)
		}
		s := string(data[offset : offset+int(strLen)])
		offset += int(strLen)
		return s, nil
	}

	// Helper to read u32 length-prefixed bytes
	readBytes := func() ([]byte, error) {
		if offset+4 > len(data) {
			return nil, fmt.Errorf("not enough data for bytes length at offset %d", offset)
		}
		bytesLen := binary.LittleEndian.Uint32(data[offset:])
		offset += 4
		if offset+int(bytesLen) > len(data) {
			return nil, fmt.Errorf("not enough data for bytes of length %d at offset %d", bytesLen, offset)
		}
		b := make([]byte, bytesLen)
		copy(b, data[offset:offset+int(bytesLen)])
		offset += int(bytesLen)
		return b, nil
	}

	result := &GMPCallResultAccount{}

	// Version (u8)
	if offset >= len(data) {
		return nil, fmt.Errorf("not enough data for version")
	}
	result.Version = data[offset]
	offset++

	// Sender (String)
	var err error
	result.Sender, err = readString()
	if err != nil {
		return nil, fmt.Errorf("reading sender: %w", err)
	}

	// Sequence (u64)
	if offset+8 > len(data) {
		return nil, fmt.Errorf("not enough data for sequence")
	}
	result.Sequence = binary.LittleEndian.Uint64(data[offset:])
	offset += 8

	// SourceClient (String)
	result.SourceClient, err = readString()
	if err != nil {
		return nil, fmt.Errorf("reading source_client: %w", err)
	}

	// DestClient (String)
	result.DestClient, err = readString()
	if err != nil {
		return nil, fmt.Errorf("reading dest_client: %w", err)
	}

	// Status (u8 enum)
	if offset >= len(data) {
		return nil, fmt.Errorf("not enough data for status")
	}
	result.Status = CallResultStatus(data[offset])
	offset++

	// Acknowledgement (Vec<u8>)
	result.Acknowledgement, err = readBytes()
	if err != nil {
		return nil, fmt.Errorf("reading acknowledgement: %w", err)
	}

	// ResultTimestamp (i64)
	if offset+8 > len(data) {
		return nil, fmt.Errorf("not enough data for result_timestamp")
	}
	result.ResultTimestamp = int64(binary.LittleEndian.Uint64(data[offset:]))
	offset += 8

	// Bump (u8)
	if offset >= len(data) {
		return nil, fmt.Errorf("not enough data for bump")
	}
	result.Bump = data[offset]

	return result, nil
}

func (s *Solana) CreateIBCAddressLookupTableAccounts(cosmosChainID string, gmpPortID string, clientID string, userPubKey solana.PublicKey) []solana.PublicKey {
	accessManagerPDA, _ := AccessManager.AccessManagerPDA(access_manager.ProgramID)
	routerStatePDA, _ := Ics26Router.RouterStatePDA(ics26_router.ProgramID)
	ibcAppPDA, _ := Ics26Router.IbcAppPDA(ics26_router.ProgramID, []byte(gmpPortID))
	gmpAppStatePDA, _ := Ics27Gmp.AppStateGmpportPDA(ics27_gmp.ProgramID)
	clientPDA, _ := Ics26Router.ClientPDA(ics26_router.ProgramID, []byte(clientID))
	clientStatePDA, _ := Ics07Tendermint.ClientPDA(ics07_tendermint.ProgramID, []byte(cosmosChainID))

	return []solana.PublicKey{
		solana.SystemProgramID,
		ComputeBudgetProgramID(),
		access_manager.ProgramID,
		solana.SysVarInstructionsPubkey,
		ics26_router.ProgramID,
		ics07_tendermint.ProgramID,
		ics27_gmp.ProgramID,
		accessManagerPDA,
		routerStatePDA,
		userPubKey,
		ibcAppPDA,
		gmpAppStatePDA,
		clientPDA,
		clientStatePDA,
	}
}
