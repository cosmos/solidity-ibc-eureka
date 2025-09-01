package solana

import (
	"context"
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
