package solana

import (
	"context"
	"encoding/binary"
	"fmt"

	solanago "github.com/gagliardetto/solana-go"
	"github.com/gagliardetto/solana-go/programs/system"
	"github.com/gagliardetto/solana-go/programs/token"
)

const (
	MintAccountSize  = uint64(82)
	TokenAccountSize = uint64(165)
)

// CreateSPLTokenMint creates a new SPL token mint with specified decimals
func (s *Solana) CreateSPLTokenMint(ctx context.Context, authority *solanago.Wallet, decimals uint8) (solanago.PublicKey, error) {
	mintAccount := solanago.NewWallet()
	mintPubkey := mintAccount.PublicKey()

	rentExemption, err := s.RPCClient.GetMinimumBalanceForRentExemption(ctx, MintAccountSize, "confirmed")
	if err != nil {
		return solanago.PublicKey{}, err
	}

	createAccountIx := system.NewCreateAccountInstruction(
		rentExemption,
		MintAccountSize,
		token.ProgramID,
		authority.PublicKey(),
		mintPubkey,
	).Build()

	initMintIx := token.NewInitializeMint2Instruction(
		decimals,
		authority.PublicKey(), // Mint authority
		authority.PublicKey(), // Freeze authority
		mintPubkey,
	).Build()

	tx, err := s.NewTransactionFromInstructions(
		authority.PublicKey(),
		createAccountIx,
		initMintIx,
	)
	if err != nil {
		return solanago.PublicKey{}, err
	}

	_, err = s.SignAndBroadcastTxWithRetry(ctx, tx, authority, mintAccount)
	if err != nil {
		return solanago.PublicKey{}, err
	}

	return mintPubkey, nil
}

// CreateTokenAccount creates a new SPL token account for the specified owner
func (s *Solana) CreateTokenAccount(ctx context.Context, payer *solanago.Wallet, mint, owner solanago.PublicKey) (solanago.PublicKey, error) {
	tokenAccount := solanago.NewWallet()
	tokenAccountPubkey := tokenAccount.PublicKey()

	rentExemption, err := s.RPCClient.GetMinimumBalanceForRentExemption(ctx, TokenAccountSize, "confirmed")
	if err != nil {
		return solanago.PublicKey{}, err
	}

	createAccountIx := system.NewCreateAccountInstruction(
		rentExemption,
		TokenAccountSize,
		token.ProgramID,
		payer.PublicKey(),
		tokenAccountPubkey,
	).Build()

	// Using InitializeAccount3 which doesn't require rent sysvar
	initAccountIx := token.NewInitializeAccount3Instruction(
		owner,
		tokenAccountPubkey,
		mint,
	).Build()

	tx, err := s.NewTransactionFromInstructions(
		payer.PublicKey(),
		createAccountIx,
		initAccountIx,
	)
	if err != nil {
		return solanago.PublicKey{}, err
	}

	_, err = s.SignAndBroadcastTxWithRetry(ctx, tx, payer, tokenAccount)
	if err != nil {
		return solanago.PublicKey{}, err
	}

	return tokenAccountPubkey, nil
}

// MintTokensTo mints tokens to a specified token account
func (s *Solana) MintTokensTo(ctx context.Context, mintAuthority *solanago.Wallet, mint, destination solanago.PublicKey, amount uint64) error {
	mintToIx := token.NewMintToInstruction(
		amount,
		mint,
		destination,
		mintAuthority.PublicKey(),
		[]solanago.PublicKey{},
	).Build()

	tx, err := s.NewTransactionFromInstructions(
		mintAuthority.PublicKey(),
		mintToIx,
	)
	if err != nil {
		return err
	}

	_, err = s.SignAndBroadcastTxWithRetry(ctx, tx, mintAuthority)
	return err
}

// GetTokenBalance retrieves the token balance for a token account
func (s *Solana) GetTokenBalance(ctx context.Context, tokenAccount solanago.PublicKey) (uint64, error) {
	accountInfo, err := s.RPCClient.GetAccountInfo(ctx, tokenAccount)
	if err != nil {
		return 0, err
	}

	if accountInfo.Value == nil {
		return 0, fmt.Errorf("token account not found")
	}

	data := accountInfo.Value.Data.GetBinary()
	if len(data) < 72 {
		return 0, fmt.Errorf("invalid token account data")
	}

	// Token balance is at offset 64 (8 bytes, little endian)
	balance := binary.LittleEndian.Uint64(data[64:72])
	return balance, nil
}
