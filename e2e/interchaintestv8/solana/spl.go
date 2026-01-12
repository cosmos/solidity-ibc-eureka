package solana

import (
	"context"
	"encoding/binary"
	"fmt"

	solanago "github.com/gagliardetto/solana-go"
	associatedtokenaccount "github.com/gagliardetto/solana-go/programs/associated-token-account"
	"github.com/gagliardetto/solana-go/programs/system"
	"github.com/gagliardetto/solana-go/programs/token"
	"github.com/gagliardetto/solana-go/rpc"
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

	// Use confirmed commitment for faster execution (optimized path: skip preflight, wait for confirmed)
	_, err = s.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentConfirmed, authority, mintAccount)
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

	// Use confirmed commitment for faster execution (optimized path: skip preflight, wait for confirmed)
	_, err = s.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentConfirmed, payer, tokenAccount)
	if err != nil {
		return solanago.PublicKey{}, err
	}

	return tokenAccountPubkey, nil
}

// AssociatedTokenAccountAddress derives the Associated Token Account address for a given owner and mint.
// This is the canonical ATA address used by the Associated Token Program.
func AssociatedTokenAccountAddress(owner, mint solanago.PublicKey) (solanago.PublicKey, error) {
	// ATA seeds: [owner, TOKEN_PROGRAM_ID, mint]
	// Program: ASSOCIATED_TOKEN_PROGRAM_ID
	associatedTokenProgramID := solanago.MustPublicKeyFromBase58("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL")

	addr, _, err := solanago.FindProgramAddress(
		[][]byte{
			owner[:],
			token.ProgramID[:],
			mint[:],
		},
		associatedTokenProgramID,
	)
	return addr, err
}

// CreateOrGetAssociatedTokenAccount creates an Associated Token Account (ATA) for the given owner and mint.
// If the ATA already exists, it returns the existing address. ATAs are deterministic addresses derived
// from the owner and mint, which IFT expects for refund processing.
func (s *Solana) CreateOrGetAssociatedTokenAccount(ctx context.Context, payer *solanago.Wallet, mint, owner solanago.PublicKey) (solanago.PublicKey, error) {
	ata, err := AssociatedTokenAccountAddress(owner, mint)
	if err != nil {
		return solanago.PublicKey{}, err
	}

	// Check if ATA already exists
	_, err = s.RPCClient.GetAccountInfo(ctx, ata)
	if err == nil {
		return ata, nil // Already exists
	}

	// Create ATA instruction
	createATAIx := associatedtokenaccount.NewCreateInstruction(
		payer.PublicKey(),
		owner,
		mint,
	).Build()

	tx, err := s.NewTransactionFromInstructions(payer.PublicKey(), createATAIx)
	if err != nil {
		return solanago.PublicKey{}, err
	}

	// Use confirmed commitment for faster execution
	_, err = s.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentConfirmed, payer)
	if err != nil {
		return solanago.PublicKey{}, err
	}

	return ata, nil
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

	// Use confirmed commitment for faster execution (optimized path: skip preflight, wait for confirmed)
	_, err = s.SignAndBroadcastTxWithRetry(ctx, tx, rpc.CommitmentConfirmed, mintAuthority)
	return err
}

// GetTokenBalance retrieves the token balance for a token account
func (s *Solana) GetTokenBalance(ctx context.Context, tokenAccount solanago.PublicKey) (uint64, error) {
	// Use confirmed commitment to match relayer read commitment level
	accountInfo, err := s.RPCClient.GetAccountInfoWithOpts(ctx, tokenAccount, &rpc.GetAccountInfoOpts{
		Commitment: rpc.CommitmentConfirmed,
	})
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
