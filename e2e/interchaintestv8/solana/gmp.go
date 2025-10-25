package solana

import (
	"context"

	bin "github.com/gagliardetto/binary"

	solanago "github.com/gagliardetto/solana-go"
)

// AccountState represents the ICS27 GMP account state PDA
// This mirrors the Rust struct in programs/solana/programs/ics27-gmp/src/state.rs
type AccountState struct {
	ClientID       string
	Sender         string
	Salt           []byte
	Nonce          uint64
	CreatedAt      int64
	LastExecutedAt int64
	ExecutionCount uint64
	Bump           uint8
}

// GetICS27AccountNonce retrieves the nonce from an ICS27 account state PDA
// Returns 0 if account doesn't exist or cannot be read
func (s *Solana) GetICS27AccountNonce(ctx context.Context, accountPDA solanago.PublicKey) uint64 {
	accountInfo, err := s.RPCClient.GetAccountInfo(ctx, accountPDA)
	if err != nil || accountInfo.Value == nil {
		return 0 // Account doesn't exist yet
	}

	data := accountInfo.Value.Data.GetBinary()
	if len(data) < 8 {
		return 0
	}

	// Create Borsh decoder and skip discriminator
	decoder := bin.NewBorshDecoder(data)
	_, err = decoder.ReadDiscriminator()
	if err != nil {
		return 0
	}

	// Decode AccountState using Borsh
	var accountState AccountState
	err = decoder.Decode(&accountState.ClientID)
	if err != nil {
		return 0
	}
	err = decoder.Decode(&accountState.Sender)
	if err != nil {
		return 0
	}
	err = decoder.Decode(&accountState.Salt)
	if err != nil {
		return 0
	}
	err = decoder.Decode(&accountState.Nonce)
	if err != nil {
		return 0
	}

	return accountState.Nonce
}
