package solana

import (
	"bytes"
	"context"
	"encoding/binary"
	"fmt"
	"slices"
	"time"

	"github.com/cosmos/solidity-ibc-eureka/e2e/v8/testvalues"
	bin "github.com/gagliardetto/binary"

	"github.com/gagliardetto/solana-go"
	"github.com/gagliardetto/solana-go/programs/system"
	"github.com/gagliardetto/solana-go/rpc"
	confirm "github.com/gagliardetto/solana-go/rpc/sendAndConfirmTransaction"
	"github.com/gagliardetto/solana-go/rpc/ws"

	"github.com/cosmos/interchaintest/v10/testutil"
)

type Solana struct {
	RPCClient *rpc.Client
	WSClient  *ws.Client
	Faucet    *solana.Wallet
}

func NewSolana(rpcURL, wsURL string, faucet *solana.Wallet) (Solana, error) {
	wsClient, err := ws.Connect(context.TODO(), wsURL)
	if err != nil {
		return Solana{}, err
	}

	return Solana{
		RPCClient: rpc.New(rpcURL),
		WSClient:  wsClient,
		Faucet:    faucet,
	}, nil
}

func NewLocalnetSolana(faucet *solana.Wallet) (Solana, error) {
	return NewSolana(rpc.LocalNet.RPC, rpc.LocalNet.WS, faucet)
}

// NewTransactionFromInstructions creates a new tx from the given transactions
func (s *Solana) NewTransactionFromInstructions(payerPubKey solana.PublicKey, instructions ...solana.Instruction) (*solana.Transaction, error) {
	recent, err := s.RPCClient.GetLatestBlockhash(context.TODO(), rpc.CommitmentFinalized)
	if err != nil {
		return nil, err
	}

	return solana.NewTransaction(
		instructions,
		recent.Value.Blockhash,
		solana.TransactionPayer(payerPubKey),
	)
}

// SignTx signs a transaction with the provided signers, broadcasts it, and confirms it.
func (s *Solana) SignAndBroadcastTx(ctx context.Context, tx *solana.Transaction, signers ...*solana.Wallet) (solana.Signature, error) {
	_, err := s.SignTx(ctx, tx, signers...)
	if err != nil {
		return solana.Signature{}, err
	}

	return s.BroadcastTx(ctx, tx)
}

// SignTx signs a transaction with the provided signers.
// It modifies the transaction in place and returns the signatures.
func (s *Solana) SignTx(ctx context.Context, tx *solana.Transaction, signers ...*solana.Wallet) ([]solana.Signature, error) {
	if len(signers) == 0 {
		return nil, fmt.Errorf("no signers provided")
	}

	signerFn := func(key solana.PublicKey) *solana.PrivateKey {
		keyIdx := slices.IndexFunc(signers, func(signer *solana.Wallet) bool {
			return signer.PublicKey().Equals(key)
		})
		if keyIdx == -1 {
			panic(fmt.Sprintf("signer %s not found in provided signers", key))
		}
		return &signers[keyIdx].PrivateKey
	}

	return tx.Sign(signerFn)
}

// Broadcasts and confirms a **signed** transaction.
func (s *Solana) BroadcastTx(ctx context.Context, tx *solana.Transaction) (solana.Signature, error) {
	return confirm.SendAndConfirmTransaction(
		ctx,
		s.RPCClient,
		s.WSClient,
		tx,
	)
}

// confirmationStatusLevel returns a numeric level for comparison.
// Higher numbers indicate higher confirmation levels.
func confirmationStatusLevel(status rpc.ConfirmationStatusType) int {
	switch status {
	case rpc.ConfirmationStatusProcessed:
		return 1
	case rpc.ConfirmationStatusConfirmed:
		return 2
	case rpc.ConfirmationStatusFinalized:
		return 3
	default:
		return 0
	}
}

func (s *Solana) WaitForTxStatus(txSig solana.Signature, status rpc.ConfirmationStatusType) error {
	return testutil.WaitForCondition(time.Second*30, time.Second, func() (bool, error) {
		out, err := s.RPCClient.GetSignatureStatuses(context.TODO(), false, txSig)
		if err != nil {
			return false, err
		}

		// // Check if transaction status exists
		// if len(out.Value) == 0 || out.Value[0] == nil {
		// 	// Transaction not yet processed
		// 	return false, nil
		// }

		if out.Value[0].Err != nil {
			return false, fmt.Errorf("transaction %s failed with error: %s", txSig, out.Value[0].Err)
		}

		// Check if transaction has reached the desired status using level-based comparison
		// This allows accepting higher confirmation levels (e.g., finalized when waiting for confirmed)
		if confirmationStatusLevel(out.Value[0].ConfirmationStatus) >= confirmationStatusLevel(status) {
			return true, nil
		}

		return false, nil
	})
}

func (s *Solana) FundUser(pubkey solana.PublicKey, amount uint64) (solana.Signature, error) {
	recent, err := s.RPCClient.GetLatestBlockhash(context.TODO(), rpc.CommitmentFinalized)
	if err != nil {
		return solana.Signature{}, err
	}

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
		return solana.Signature{}, err
	}

	return s.SignAndBroadcastTxWithConfirmedStatus(context.TODO(), tx, s.Faucet)
}

func (s *Solana) CreateAndFundWallet() (*solana.Wallet, error) {
	wallet := solana.NewWallet()
	if _, err := s.FundUser(wallet.PublicKey(), testvalues.InitialSolBalance); err != nil {
		return nil, err
	}
	return wallet, nil
}

// WaitForProgramAvailability waits for a program to become available with default timeout
func (s *Solana) WaitForProgramAvailability(ctx context.Context, programID solana.PublicKey) bool {
	return s.WaitForProgramAvailabilityWithTimeout(ctx, programID, 30)
}

// WaitForProgramAvailabilityWithTimeout waits for a program to become available with specified timeout
func (s *Solana) WaitForProgramAvailabilityWithTimeout(ctx context.Context, programID solana.PublicKey, timeoutSeconds int) bool {
	for range timeoutSeconds {
		accountInfo, err := s.RPCClient.GetAccountInfo(ctx, programID)
		if err == nil && accountInfo.Value != nil && accountInfo.Value.Executable {
			return true
		}
		time.Sleep(1 * time.Second)
	}
	return false
}

// SignAndBroadcastTxWithRetry retries transaction broadcasting with default timeout
func (s *Solana) SignAndBroadcastTxWithRetry(ctx context.Context, tx *solana.Transaction, signers ...*solana.Wallet) (solana.Signature, error) {
	return s.SignAndBroadcastTxWithRetryTimeout(ctx, tx, 30, signers...)
}

// SignAndBroadcastTxWithRetryTimeout retries transaction broadcasting with specified timeout
// It refreshes the blockhash on each attempt to handle expired blockhashes
func (s *Solana) SignAndBroadcastTxWithRetryTimeout(ctx context.Context, tx *solana.Transaction, timeoutSeconds int, signers ...*solana.Wallet) (solana.Signature, error) {
	var lastErr error
	for range timeoutSeconds {
		// Refresh blockhash on each retry attempt (blockhashes expire after ~60 seconds)
		recent, err := s.RPCClient.GetLatestBlockhash(ctx, rpc.CommitmentFinalized)
		if err != nil {
			lastErr = fmt.Errorf("failed to get latest blockhash: %w", err)
			time.Sleep(1 * time.Second)
			continue
		}
		tx.Message.RecentBlockhash = recent.Value.Blockhash

		sig, err := s.SignAndBroadcastTx(ctx, tx, signers...)
		if err == nil {
			return sig, nil
		}
		lastErr = err
		time.Sleep(1 * time.Second)
	}
	return solana.Signature{}, fmt.Errorf("transaction broadcast timed out after %d seconds: %w", timeoutSeconds, lastErr)
}

func (s *Solana) SignAndBroadcastTxWithConfirmedStatus(ctx context.Context, tx *solana.Transaction, wallet *solana.Wallet) (solana.Signature, error) {
	return s.SignAndBroadcastTxWithOpts(ctx, tx, wallet, rpc.ConfirmationStatusConfirmed)
}

func (s *Solana) SignAndBroadcastTxWithOpts(ctx context.Context, tx *solana.Transaction, wallet *solana.Wallet, status rpc.ConfirmationStatusType) (solana.Signature, error) {
	_, err := s.SignTx(ctx, tx, wallet)
	if err != nil {
		return solana.Signature{}, err
	}

	sig, err := s.RPCClient.SendTransactionWithOpts(
		ctx,
		tx,
		rpc.TransactionOpts{
			SkipPreflight: true,
		},
	)
	if err != nil {
		return solana.Signature{}, err
	}

	err = s.WaitForTxStatus(sig, status)
	if err != nil {
		return solana.Signature{}, err
	}

	return sig, err
}

// WaitForBalanceChange waits for an account balance to change from the initial value
func (s *Solana) WaitForBalanceChange(ctx context.Context, account solana.PublicKey, initialBalance uint64) (uint64, bool) {
	return s.WaitForBalanceChangeWithTimeout(ctx, account, initialBalance, 30)
}

// WaitForBalanceChangeWithTimeout waits for an account balance to change with specified timeout
func (s *Solana) WaitForBalanceChangeWithTimeout(ctx context.Context, account solana.PublicKey, initialBalance uint64, timeoutSeconds int) (uint64, bool) {
	for range timeoutSeconds {
		balanceResp, err := s.RPCClient.GetBalance(ctx, account, rpc.CommitmentConfirmed)
		if err == nil {
			currentBalance := balanceResp.Value
			if currentBalance != initialBalance {
				return currentBalance, true
			}
		}
		time.Sleep(1 * time.Second)
	}
	return initialBalance, false
}

// ComputeBudgetProgramID returns the Solana Compute Budget program ID
func ComputeBudgetProgramID() solana.PublicKey {
	return solana.MustPublicKeyFromBase58("ComputeBudget111111111111111111111111111111")
}

// NewComputeBudgetInstruction creates a SetComputeUnitLimit instruction to increase available compute units
func NewComputeBudgetInstruction(computeUnits uint32) solana.Instruction {
	data := make([]byte, 5)
	data[0] = 0x02 // SetComputeUnitLimit instruction discriminator
	binary.LittleEndian.PutUint32(data[1:], computeUnits)

	return solana.NewInstruction(
		ComputeBudgetProgramID(),
		solana.AccountMetaSlice{},
		data,
	)
}

// CreateAddressLookupTable creates an Address Lookup Table and extends it with the given accounts.
// Returns the ALT address. Requires at least one account.
func (s *Solana) CreateAddressLookupTable(ctx context.Context, authority *solana.Wallet, accounts []solana.PublicKey) (solana.PublicKey, error) {
	if len(accounts) == 0 {
		return solana.PublicKey{}, fmt.Errorf("at least one account is required for ALT")
	}

	// Get recent slot for ALT creation
	slot, err := s.RPCClient.GetSlot(ctx, "confirmed")
	if err != nil {
		return solana.PublicKey{}, fmt.Errorf("failed to get slot: %w", err)
	}

	// Derive ALT address with bump seed
	// The derivation uses: [authority, recent_slot] seeds
	altAddress, bumpSeed, err := solana.FindProgramAddress(
		[][]byte{authority.PublicKey().Bytes(), Uint64ToLeBytes(slot)},
		solana.AddressLookupTableProgramID,
	)
	if err != nil {
		return solana.PublicKey{}, fmt.Errorf("failed to derive ALT address: %w", err)
	}

	// Create ALT instruction data
	// ProgramInstruction enum: CreateLookupTable { recent_slot: u64, bump_seed: u8 }
	var createBuf bytes.Buffer
	encoder := bin.NewBinEncoder(&createBuf)
	mustWrite(encoder.WriteUint32(0, bin.LE))
	mustWrite(encoder.WriteUint64(slot, bin.LE))
	mustWrite(encoder.WriteUint8(bumpSeed))
	createInstructionData := createBuf.Bytes()

	createAltIx := solana.NewInstruction(
		solana.AddressLookupTableProgramID,
		solana.AccountMetaSlice{
			solana.Meta(altAddress).WRITE(),                     // lookup_table (to be created)
			solana.Meta(authority.PublicKey()).WRITE().SIGNER(), // authority
			solana.Meta(authority.PublicKey()).WRITE().SIGNER(), // payer
			solana.Meta(solana.SystemProgramID),                 // system_program
		},
		createInstructionData,
	)

	// Create ALT
	createTx, err := s.NewTransactionFromInstructions(authority.PublicKey(), createAltIx)
	if err != nil {
		return solana.PublicKey{}, fmt.Errorf("failed to create ALT transaction: %w", err)
	}

	_, err = s.SignAndBroadcastTxWithRetry(ctx, createTx, authority)
	if err != nil {
		return solana.PublicKey{}, fmt.Errorf("failed to create ALT: %w", err)
	}

	// Extend ALT with accounts instruction data
	// ProgramInstruction::ExtendLookupTable { new_addresses: Vec<Pubkey> }
	var extendBuf bytes.Buffer
	extendEncoder := bin.NewBinEncoder(&extendBuf)
	mustWrite(extendEncoder.WriteUint32(2, bin.LE))
	mustWrite(extendEncoder.WriteUint64(uint64(len(accounts)), bin.LE))
	for _, acc := range accounts {
		mustWrite(extendEncoder.WriteBytes(acc.Bytes(), false))
	}
	extendInstructionData := extendBuf.Bytes()

	extendAltIx := solana.NewInstruction(
		solana.AddressLookupTableProgramID,
		solana.AccountMetaSlice{
			solana.Meta(altAddress).WRITE(),                     // lookup_table
			solana.Meta(authority.PublicKey()).WRITE().SIGNER(), // authority
			solana.Meta(authority.PublicKey()).WRITE().SIGNER(), // payer (for reallocation)
			solana.Meta(solana.SystemProgramID),                 // system_program
		},
		extendInstructionData,
	)

	extendTx, err := s.NewTransactionFromInstructions(authority.PublicKey(), extendAltIx)
	if err != nil {
		return solana.PublicKey{}, fmt.Errorf("failed to create extend ALT transaction: %w", err)
	}

	_, err = s.SignAndBroadcastTxWithRetry(ctx, extendTx, authority)
	if err != nil {
		return solana.PublicKey{}, fmt.Errorf("failed to extend ALT: %w", err)
	}

	return altAddress, nil
}

// Uint64ToLeBytes converts a uint64 to little-endian byte slice
func Uint64ToLeBytes(n uint64) []byte {
	b := make([]byte, 8)
	binary.LittleEndian.PutUint64(b, n)
	return b
}

// mustWrite wraps encoder write calls and panics on error (should never happen with bytes.Buffer)
func mustWrite(err error) {
	if err != nil {
		panic(fmt.Sprintf("unexpected encoding error: %v", err))
	}
}
