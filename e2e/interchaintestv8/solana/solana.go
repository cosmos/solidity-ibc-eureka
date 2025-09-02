package solana

import (
	"context"
	"encoding/binary"
	"fmt"
	"slices"
	"time"

	"github.com/cosmos/solidity-ibc-eureka/e2e/v8/testvalues"

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

func (s *Solana) WaitForTxConfirmation(txSig solana.Signature) error {
	return s.WaitForTxStatus(txSig, rpc.ConfirmationStatusConfirmed)
}

func (s *Solana) WaitForTxFinalization(txSig solana.Signature) error {
	return s.WaitForTxStatus(txSig, rpc.ConfirmationStatusFinalized)
}

func (s *Solana) WaitForTxStatus(txSig solana.Signature, status rpc.ConfirmationStatusType) error {
	return testutil.WaitForCondition(time.Second*30, time.Second, func() (bool, error) {
		out, err := s.RPCClient.GetSignatureStatuses(context.TODO(), false, txSig)
		if err != nil {
			return false, err
		}

		if out.Value[0].Err != nil {
			return false, fmt.Errorf("transaction %s failed with error: %s", txSig, out.Value[0].Err)
		}

		if out.Value[0].ConfirmationStatus == status {
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

	return s.SignAndBroadcastTx(context.TODO(), tx, s.Faucet)
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
func (s *Solana) SignAndBroadcastTxWithRetry(ctx context.Context, tx *solana.Transaction, wallet *solana.Wallet) (solana.Signature, error) {
	return s.SignAndBroadcastTxWithRetryTimeout(ctx, tx, wallet, 30)
}

// SignAndBroadcastTxWithRetryTimeout retries transaction broadcasting with specified timeout
func (s *Solana) SignAndBroadcastTxWithRetryTimeout(ctx context.Context, tx *solana.Transaction, wallet *solana.Wallet, timeoutSeconds int) (solana.Signature, error) {
	var lastErr error
	for range timeoutSeconds {
		sig, err := s.SignAndBroadcastTx(ctx, tx, wallet)
		if err == nil {
			return sig, nil
		}
		lastErr = err
		time.Sleep(1 * time.Second)
	}
	return solana.Signature{}, fmt.Errorf("transaction broadcast timed out after %d seconds: %w", timeoutSeconds, lastErr)
}

// WaitForBalanceChange waits for an account balance to change from the initial value
func (s *Solana) WaitForBalanceChange(ctx context.Context, account solana.PublicKey, initialBalance uint64) (uint64, bool) {
	return s.WaitForBalanceChangeWithTimeout(ctx, account, initialBalance, 30)
}

// WaitForBalanceChangeWithTimeout waits for an account balance to change with specified timeout
func (s *Solana) WaitForBalanceChangeWithTimeout(ctx context.Context, account solana.PublicKey, initialBalance uint64, timeoutSeconds int) (uint64, bool) {
	for range timeoutSeconds {
		balanceResp, err := s.RPCClient.GetBalance(ctx, account, "confirmed")
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

// NewComputeBudgetInstruction creates a SetComputeUnitLimit instruction to increase available compute units
func NewComputeBudgetInstruction(computeUnits uint32) solana.Instruction {
	// Compute Budget Program ID
	computeBudgetProgramID := solana.MustPublicKeyFromBase58("ComputeBudget111111111111111111111111111111")
	data := make([]byte, 5)
	data[0] = 0x02 // SetComputeUnitLimit instruction discriminator
	binary.LittleEndian.PutUint32(data[1:], computeUnits)

	return solana.NewInstruction(
		computeBudgetProgramID,
		solana.AccountMetaSlice{},
		data,
	)
}
