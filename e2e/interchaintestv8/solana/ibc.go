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

func (s *Solana) CreateIBCAddressLookupTableAccounts(cosmosChainID string, gmpPortID string, clientID string, userPubKey solana.PublicKey) []solana.PublicKey {
	accessManagerPDA, _ := AccessManager.AccessManagerPDA(access_manager.ProgramID)
	routerStatePDA, _ := Ics26Router.RouterStatePDA(ics26_router.ProgramID)
	ibcAppPDA, _ := Ics26Router.IbcAppWithArgSeedPDA(ics26_router.ProgramID, []byte(gmpPortID))
	gmpAppStatePDA, _ := Ics27Gmp.AppStateGmpportPDA(ics27_gmp.ProgramID)
	clientPDA, _ := Ics26Router.ClientWithArgSeedPDA(ics26_router.ProgramID, []byte(clientID))
	clientStatePDA, _ := Ics07Tendermint.ClientWithArgSeedPDA(ics07_tendermint.ProgramID, []byte(cosmosChainID))

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
