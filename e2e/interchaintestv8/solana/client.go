package solana

import (
	"bytes"
	"context"
	"encoding/binary"
	"fmt"
	"slices"
	"time"

	bin "github.com/gagliardetto/binary"

	"github.com/gagliardetto/solana-go"
	"github.com/gagliardetto/solana-go/programs/system"
	"github.com/gagliardetto/solana-go/rpc"
	confirm "github.com/gagliardetto/solana-go/rpc/sendAndConfirmTransaction"
	"github.com/gagliardetto/solana-go/rpc/ws"

	"github.com/cosmos/interchaintest/v10/testutil"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
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

func (s *Solana) SignAndBroadcastTx(ctx context.Context, tx *solana.Transaction, signers ...*solana.Wallet) (solana.Signature, error) {
	_, err := s.SignTx(ctx, tx, signers...)
	if err != nil {
		return solana.Signature{}, err
	}

	return s.BroadcastTx(ctx, tx)
}

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

func (s *Solana) BroadcastTx(ctx context.Context, tx *solana.Transaction) (solana.Signature, error) {
	return confirm.SendAndConfirmTransaction(
		ctx,
		s.RPCClient,
		s.WSClient,
		tx,
	)
}

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

		if out.Value[0].Err != nil {
			return false, fmt.Errorf("transaction %s failed with error: %s", txSig, out.Value[0].Err)
		}

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

func (s *Solana) SignAndBroadcastTxWithRetry(ctx context.Context, tx *solana.Transaction, signers ...*solana.Wallet) (solana.Signature, error) {
	return s.SignAndBroadcastTxWithRetryTimeout(ctx, tx, 30, signers...)
}

func (s *Solana) SignAndBroadcastTxWithRetryTimeout(ctx context.Context, tx *solana.Transaction, timeoutSeconds int, signers ...*solana.Wallet) (solana.Signature, error) {
	var lastErr error
	for range timeoutSeconds {
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

func (s *Solana) WaitForBalanceChange(ctx context.Context, account solana.PublicKey, initialBalance uint64) (uint64, bool) {
	return s.WaitForBalanceChangeWithTimeout(ctx, account, initialBalance, 30)
}

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

func ComputeBudgetProgramID() solana.PublicKey {
	return solana.MustPublicKeyFromBase58("ComputeBudget111111111111111111111111111111")
}

func NewComputeBudgetInstruction(computeUnits uint32) solana.Instruction {
	data := make([]byte, 5)
	data[0] = 0x02
	binary.LittleEndian.PutUint32(data[1:], computeUnits)

	return solana.NewInstruction(
		ComputeBudgetProgramID(),
		solana.AccountMetaSlice{},
		data,
	)
}

func (s *Solana) CreateAddressLookupTable(ctx context.Context, authority *solana.Wallet, accounts []solana.PublicKey) (solana.PublicKey, error) {
	if len(accounts) == 0 {
		return solana.PublicKey{}, fmt.Errorf("at least one account is required for ALT")
	}

	slot, err := s.RPCClient.GetSlot(ctx, rpc.CommitmentProcessed)
	if err != nil {
		return solana.PublicKey{}, fmt.Errorf("failed to get slot: %w", err)
	}

	altAddress, bumpSeed := AddressLookupTablePDA(authority.PublicKey(), slot)

	var createBuf bytes.Buffer
	encoder := bin.NewBinEncoder(&createBuf)
	mustWrite(encoder.WriteUint32(0, bin.LE))
	mustWrite(encoder.WriteUint64(slot, bin.LE))
	mustWrite(encoder.WriteUint8(bumpSeed))
	createInstructionData := createBuf.Bytes()

	createAltIx := solana.NewInstruction(
		solana.AddressLookupTableProgramID,
		solana.AccountMetaSlice{
			solana.Meta(altAddress).WRITE(),
			solana.Meta(authority.PublicKey()).WRITE().SIGNER(),
			solana.Meta(authority.PublicKey()).WRITE().SIGNER(),
			solana.Meta(solana.SystemProgramID),
		},
		createInstructionData,
	)

	createTx, err := s.NewTransactionFromInstructions(authority.PublicKey(), createAltIx)
	if err != nil {
		return solana.PublicKey{}, fmt.Errorf("failed to create ALT transaction: %w", err)
	}

	_, err = s.SignAndBroadcastTxWithRetry(ctx, createTx, authority)
	if err != nil {
		return solana.PublicKey{}, fmt.Errorf("failed to create ALT: %w", err)
	}

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
			solana.Meta(altAddress).WRITE(),
			solana.Meta(authority.PublicKey()).WRITE().SIGNER(),
			solana.Meta(authority.PublicKey()).WRITE().SIGNER(),
			solana.Meta(solana.SystemProgramID),
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

func mustWrite(err error) {
	if err != nil {
		panic(fmt.Sprintf("unexpected encoding error: %v", err))
	}
}

func (s *Solana) GetSolanaClockTime(ctx context.Context) (int64, error) {
	clockSysvarPubkey := solana.MustPublicKeyFromBase58("SysvarC1ock11111111111111111111111111111111")

	accountInfo, err := s.RPCClient.GetAccountInfo(ctx, clockSysvarPubkey)
	if err != nil {
		return 0, fmt.Errorf("failed to get clock sysvar account: %w", err)
	}
	if accountInfo.Value == nil {
		return 0, fmt.Errorf("clock sysvar account is nil")
	}

	data := accountInfo.Value.Data.GetBinary()
	if len(data) < 40 {
		return 0, fmt.Errorf("clock sysvar data too short: expected >= 40 bytes, got %d", len(data))
	}

	unixTimestamp := int64(binary.LittleEndian.Uint64(data[32:40]))
	return unixTimestamp, nil
}
