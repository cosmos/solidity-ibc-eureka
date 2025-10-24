package solana

import (
	"context"
	"fmt"
	"time"

	"github.com/gagliardetto/solana-go"
	"github.com/gagliardetto/solana-go/programs/system"
	"github.com/gagliardetto/solana-go/rpc"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
)

// WaitForClusterReady waits for the Solana cluster to be fully initialized
func (s *Solana) WaitForClusterReady(ctx context.Context, timeout time.Duration) error {
	deadline := time.Now().Add(timeout)

	for time.Now().Before(deadline) {
		// Check 1: Can we get the latest blockhash?
		_, err := s.RPCClient.GetLatestBlockhash(ctx, rpc.CommitmentFinalized)
		if err != nil {
			time.Sleep(1 * time.Second)
			continue
		}

		// Check 2: Can we get the slot?
		slot, err := s.RPCClient.GetSlot(ctx, rpc.CommitmentFinalized)
		if err != nil || slot < 5 {
			time.Sleep(1 * time.Second)
			continue
		}

		// Check 3: Is the faucet account funded and available?
		if s.Faucet != nil {
			balance, err := s.RPCClient.GetBalance(ctx, s.Faucet.PublicKey(), rpc.CommitmentFinalized)
			if err != nil {
				time.Sleep(1 * time.Second)
				continue
			}

			// Ensure faucet has at least 10 SOL for funding operations
			minBalance := uint64(10_000_000_000) // 1000 SOL in lamports
			if balance.Value < minBalance {
				return fmt.Errorf("faucet balance too low: %d lamports (need at least %d). Re-create solana validator node", balance.Value, minBalance)
			}
		}

		// Check 4: Can we get the version? (ensures RPC is fully responsive)
		_, err = s.RPCClient.GetVersion(ctx)
		if err != nil {
			time.Sleep(1 * time.Second)
			continue
		}

		// All checks passed
		return nil
	}

	return fmt.Errorf("cluster not ready after %v", timeout)
}

// CreateAndFundWalletWithRetry creates a wallet with retry logic
func (s *Solana) CreateAndFundWalletWithRetry(ctx context.Context, retries int) (*solana.Wallet, error) {
	var lastErr error

	for i := range retries {
		// Wait a bit before retry (except first attempt)
		if i > 0 {
			time.Sleep(time.Duration(i) * time.Second)
		}

		wallet := solana.NewWallet()

		// Try to fund the wallet
		_, err := s.FundUserWithRetry(ctx, wallet.PublicKey(), testvalues.InitialSolBalance, 3)
		if err == nil {
			// Verify the balance was actually credited
			balance, err := s.RPCClient.GetBalance(ctx, wallet.PublicKey(), rpc.CommitmentConfirmed)
			if err == nil && balance.Value > 0 {
				return wallet, nil
			}
		}

		lastErr = err
	}

	return nil, fmt.Errorf("failed to create and fund wallet after %d retries: %w", retries, lastErr)
}

// FundUserWithRetry funds a user with retry logic
func (s *Solana) FundUserWithRetry(ctx context.Context, pubkey solana.PublicKey, amount uint64, retries int) (solana.Signature, error) {
	var lastErr error

	for i := range retries {
		// Wait a bit before retry (except first attempt)
		if i > 0 {
			time.Sleep(time.Duration(i) * time.Second)
		}

		// Check faucet balance first
		faucetBalance, err := s.RPCClient.GetBalance(ctx, s.Faucet.PublicKey(), rpc.CommitmentConfirmed)
		if err != nil {
			lastErr = fmt.Errorf("failed to get faucet balance: %w", err)
			continue
		}

		if faucetBalance.Value < amount {
			lastErr = fmt.Errorf("insufficient faucet balance: %d < %d", faucetBalance.Value, amount)
			continue
		}

		// Get latest blockhash
		recent, err := s.RPCClient.GetLatestBlockhash(ctx, rpc.CommitmentFinalized)
		if err != nil {
			lastErr = fmt.Errorf("failed to get blockhash: %w", err)
			continue
		}

		// Create transfer transaction
		tx, err := solana.NewTransaction(
			[]solana.Instruction{
				system.NewTransferInstruction(
					amount,
					s.Faucet.PublicKey(),
					pubkey,
				).Build(),
			},
			recent.Value.Blockhash,
			solana.TransactionPayer(s.Faucet.PublicKey()),
		)
		if err != nil {
			lastErr = fmt.Errorf("failed to create transaction: %w", err)
			continue
		}

		// Sign and broadcast
		sig, err := s.SignAndBroadcastTx(ctx, tx, s.Faucet)
		if err == nil {
			// Wait for confirmation
			time.Sleep(2 * time.Second)

			// Verify the transfer succeeded
			balance, err := s.RPCClient.GetBalance(ctx, pubkey, rpc.CommitmentConfirmed)
			if err == nil && balance.Value >= amount {
				return sig, nil
			}
		}

		lastErr = err
	}

	return solana.Signature{}, fmt.Errorf("failed to fund user after %d retries: %w", retries, lastErr)
}
