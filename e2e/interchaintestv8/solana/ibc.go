package solana

import (
	"context"
	"encoding/binary"
	"fmt"

	"github.com/gagliardetto/solana-go"
	"github.com/gagliardetto/solana-go/rpc"

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

func (s *Solana) CreateIBCAddressLookupTableAccounts(cosmosChainID string, gmpPortID string, clientID string, userPubKey solana.PublicKey) []solana.PublicKey {
	routerStatePDA, _ := Ics26Router.RouterStatePDA(ics26_router.ProgramID)
	ibcAppPDA, _ := Ics26Router.IbcAppPDA(ics26_router.ProgramID, []byte(gmpPortID))
	gmpAppStatePDA, _ := Ics27Gmp.AppStateGmpportPDA(ics27_gmp.ProgramID)
	clientPDA, _ := Ics26Router.ClientPDA(ics26_router.ProgramID, []byte(clientID))
	clientStatePDA, _ := Ics07Tendermint.ClientPDA(ics07_tendermint.ProgramID, []byte(cosmosChainID))
	routerCallerPDA, _ := Ics27Gmp.RouterCallerPDA(ics27_gmp.ProgramID)
	clientSequencePDA, _ := Ics26Router.ClientSequencePDA(ics26_router.ProgramID, []byte(clientID))

	return []solana.PublicKey{
		solana.SystemProgramID,
		ComputeBudgetProgramID(),
		ics26_router.ProgramID,
		ics07_tendermint.ProgramID,
		ics27_gmp.ProgramID,
		routerStatePDA,
		userPubKey,
		ibcAppPDA,
		gmpAppStatePDA,
		clientPDA,
		clientStatePDA,
		routerCallerPDA,
		clientSequencePDA,
	}
}
