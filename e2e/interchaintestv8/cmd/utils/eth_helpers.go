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

func GetTransactOpts(ctx context.Context, ethClient *ethclient.Client, chainID *big.Int, key *ecdsa.PrivateKey, extraGwei int64) *bind.TransactOpts {
	fromAddress := crypto.PubkeyToAddress(key.PublicKey)

	txOpts, err := bind.NewKeyedTransactorWithChainID(key, chainID)
	if err != nil {
		panic(err)
	}

	// Get the suggested gas price from the client.
	suggestedGasPrice, err := ethClient.SuggestGasPrice(ctx)
	if err != nil {
		panic(err)
	}

	txOpts.GasPrice = new(big.Int).Add(suggestedGasPrice, big.NewInt(extraGwei*1000000000)) // Add extra Gwei

	nonce, err := ethClient.PendingNonceAt(context.Background(), fromAddress)
	if err != nil {
		nonce = 0
	}
	txOpts.Nonce = big.NewInt(int64(nonce))

	// header, err := ethClient.HeaderByNumber(ctx, nil)
	// if err != nil {
	// 	panic(err)
	// }
	//
	// // For EIP-1559 transactions: double the gas tip and fee cap.
	// tipCap, err := ethClient.SuggestGasTipCap(ctx)
	// if err != nil {
	// 	panic(err)
	// }
	// txOpts.GasTipCap = new(big.Int).Mul(tipCap, big.NewInt(5))
	// // Compute the gas fee cap by doubling the sum of header.BaseFee and the original tipCap.
	// txOpts.GasFeeCap = new(big.Int).Mul(new(big.Int).Add(header.BaseFee, tipCap), big.NewInt(5))

	return txOpts
}

func GetTxReceipt(ctx context.Context, ethClient *ethclient.Client, hash ethcommon.Hash) *ethtypes.Receipt {
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
