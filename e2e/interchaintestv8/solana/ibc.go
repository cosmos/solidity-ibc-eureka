package solana

import (
	"context"
	"encoding/binary"
	"fmt"

	"github.com/gagliardetto/solana-go"

	ics07_tendermint "github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/ics07tendermint"
	ics26_router "github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/ics26router"
	ics27_gmp "github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/ics27gmp"
)

func (s *Solana) GetNextSequenceNumber(ctx context.Context, clientSequencePDA solana.PublicKey) (uint64, error) {
	clientSequenceAccount, err := s.RPCClient.GetAccountInfo(ctx, clientSequencePDA)
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
	routerStatePDA, _ := Ics26RouterRouterStatePDA(ics26_router.ProgramID)
	ibcAppPDA, _ := Ics26RouterIbcAppPDA(ics26_router.ProgramID, []byte(gmpPortID))
	gmpAppStatePDA, _ := Ics27GmpAppStateGmpportPDA(ics27_gmp.ProgramID)
	clientPDA, _ := Ics26RouterClientPDA(ics26_router.ProgramID, []byte(clientID))
	clientStatePDA, _ := Ics07TendermintClientPDA(ics07_tendermint.ProgramID, []byte(cosmosChainID))
	routerCallerPDA, _ := Ics27GmpRouterCallerPDA(ics27_gmp.ProgramID)
	clientSequencePDA, _ := Ics26RouterClientSequencePDA(ics26_router.ProgramID, []byte(clientID))

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
