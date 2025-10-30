package solana

import (
	"context"

	solanago "github.com/gagliardetto/solana-go"
	"github.com/gagliardetto/solana-go/rpc"

	ics27_gmp "github.com/cosmos/solidity-ibc-eureka/packages/go-anchor/ics27gmp"
)

// GetICS27AccountNonce retrieves the nonce from an ICS27 account state PDA
// Returns 0 if account doesn't exist or cannot be read
func (s *Solana) GetICS27AccountNonce(ctx context.Context, accountPDA solanago.PublicKey) uint64 {
	// Use confirmed commitment to match relayer read commitment level
	accountInfo, err := s.RPCClient.GetAccountInfoWithOpts(ctx, accountPDA, &rpc.GetAccountInfoOpts{
		Commitment: rpc.CommitmentConfirmed,
	})
	if err != nil || accountInfo.Value == nil {
		return 0 // Account doesn't exist yet
	}

	data := accountInfo.Value.Data.GetBinary()
	if len(data) < 8 {
		return 0
	}

	// Parse using auto-generated anchor-go parser
	accountState, err := ics27_gmp.ParseAccount_Ics27GmpStateAccountState(data)
	if err != nil {
		return 0
	}

	return accountState.Nonce
}
