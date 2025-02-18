package utils

import (
	"context"
	"crypto/ecdsa"
	"encoding/hex"
	"math/big"
	"time"

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

func GetTransactOpts(ethClient *ethclient.Client, chainID *big.Int, key *ecdsa.PrivateKey) *bind.TransactOpts {
	fromAddress := crypto.PubkeyToAddress(key.PublicKey)
	nonce, err := ethClient.PendingNonceAt(context.Background(), fromAddress)
	if err != nil {
		nonce = 0
	}

	gasPrice, err := ethClient.SuggestGasPrice(context.Background())
	if err != nil {
		panic(err)
	}

	txOpts, err := bind.NewKeyedTransactorWithChainID(key, chainID)
	if err != nil {
		panic(err)
	}

	txOpts.Nonce = big.NewInt(int64(nonce))
	txOpts.GasPrice = gasPrice

	return txOpts
}

func GetTxReciept(ctx context.Context, ethClient *ethclient.Client, hash ethcommon.Hash) *ethtypes.Receipt {
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
