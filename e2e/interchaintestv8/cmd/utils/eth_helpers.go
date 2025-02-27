package utils

import (
	"context"
	"crypto/ecdsa"
	"encoding/hex"
	"math/big"
	"time"

	"github.com/briandowns/spinner"
	"github.com/ethereum/go-ethereum/accounts/abi/bind"
	ethcommon "github.com/ethereum/go-ethereum/common"
	ethtypes "github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/crypto"
	"github.com/ethereum/go-ethereum/ethclient"
	"github.com/strangelove-ventures/interchaintest/v8/testutil"
)

func EthPrivateKeyFromHex(hexKey string) *ecdsa.PrivateKey {
	keyBytes, err := hex.DecodeString(hexKey)
	if err != nil {
		panic(err)
	}

	privateKey, err := crypto.ToECDSA(keyBytes)
	if err != nil {
		panic(err)
	}

	return privateKey
}

func GetTransactOpts(ctx context.Context, ethClient *ethclient.Client, chainID *big.Int, key *ecdsa.PrivateKey) *bind.TransactOpts {
	fromAddress := crypto.PubkeyToAddress(key.PublicKey)

	nonce, err := ethClient.PendingNonceAt(context.Background(), fromAddress)
	if err != nil {
		nonce = 0
	}

	suggestedGas, err := ethClient.SuggestGasPrice(ctx)
	if err != nil {
		panic(err)
	}

	txOpts, err := bind.NewKeyedTransactorWithChainID(key, chainID)
	if err != nil {
		panic(err)
	}

	txOpts.GasLimit = 1_500_000

	header, err := ethClient.HeaderByNumber(ctx, nil)
	if err != nil {
		panic(err)
	}
	if header.BaseFee != nil {
		// Use EIP-1559 fields like auth.GasFeeCap and auth.GasTipCap
		suggestedTip, err := ethClient.SuggestGasTipCap(ctx)
		if err != nil {
			panic(err)
		}
		// Add a 10% premium to the suggested tip
		premium := new(big.Int).Div(new(big.Int).Mul(suggestedTip, big.NewInt(10)), big.NewInt(100))
		gasTipCap := new(big.Int).Add(suggestedTip, premium)

		// GasFeeCap should cover the base fee (from the block header) plus your priority fee.
		// Here we double the base fee as a simple strategy, then add the tip.
		gasFeeCap := new(big.Int).Mul(header.BaseFee, big.NewInt(2))
		gasFeeCap.Add(gasFeeCap, gasTipCap)

		txOpts.GasTipCap = gasTipCap
		txOpts.GasFeeCap = gasFeeCap
	} else {

		suggestedTip, err := ethClient.SuggestGasTipCap(ctx)
		if err != nil {
			panic(err)
		}
		// Add a 10% premium to the suggested tip
		premiumTip := new(big.Int).Div(new(big.Int).Mul(suggestedTip, big.NewInt(10)), big.NewInt(100))
		txOpts.GasTipCap = new(big.Int).Add(suggestedTip, premiumTip)

		txOpts.Nonce = big.NewInt(int64(nonce))
		premiumGas := new(big.Int).Div(new(big.Int).Mul(suggestedGas, big.NewInt(10)), big.NewInt(100))
		txOpts.GasPrice = new(big.Int).Add(suggestedGas, premiumGas)
	}

	return txOpts
}

func GetTxReciept(ctx context.Context, ethClient *ethclient.Client, hash ethcommon.Hash) *ethtypes.Receipt {
	s := spinner.New(spinner.CharSets[14], 100*time.Millisecond)
	s.Start()
	defer s.Stop()

	var receipt *ethtypes.Receipt
	if err := testutil.WaitForCondition(time.Second*120, time.Second, func() (bool, error) {
		var err error
		receipt, err = ethClient.TransactionReceipt(ctx, hash)
		if err != nil {
			return false, nil
		}

		return receipt != nil, nil
	}); err != nil {
		return nil
	}

	return receipt
}
