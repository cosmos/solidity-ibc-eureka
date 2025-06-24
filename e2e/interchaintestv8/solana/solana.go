package solana

import (
	"context"
	"fmt"
	"time"

	"github.com/gagliardetto/solana-go"
	"github.com/gagliardetto/solana-go/rpc"
	"github.com/gagliardetto/solana-go/rpc/ws"

	"github.com/strangelove-ventures/interchaintest/v8/testutil"

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

func (s *Solana) FundUser(pubkey solana.PublicKey, amount uint64) error {
	txSig, err := s.RPCClient.RequestAirdrop(
		context.TODO(),
		pubkey,
		solana.LAMPORTS_PER_SOL*amount,
		rpc.CommitmentFinalized,
	)
	if err != nil {
		return err
	}

	return s.WaitForTxConfirmation(txSig)
}

func (s *Solana) CreateAndFundWallet() (*solana.Wallet, error) {
	wallet := solana.NewWallet()
	if err := s.FundUser(wallet.PublicKey(), testvalues.InitialSolBalance); err != nil {
		return nil, err
	}
	return wallet, nil
}
