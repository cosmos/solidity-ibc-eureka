package ethereum

import (
	"strconv"
	"time"

	"github.com/ethereum/go-ethereum/ethclient"
)

type EthAPI struct {
	client *ethclient.Client

	Retries   int
	RetryWait time.Duration
}

type EthGetProofResponse struct {
	StorageHash  string `json:"storageHash"`
	StorageProof []struct {
		Key   string   `json:"key"`
		Proof []string `json:"proof"`
		Value string   `json:"value"`
	} `json:"storageProof"`
	AccountProof []string `json:"accountProof"`
}

func NewEthAPI(rpc string) (EthAPI, error) {
	ethClient, err := ethclient.Dial(rpc)
	if err != nil {
		return EthAPI{}, err
	}

	return EthAPI{
		client:    ethClient,
		Retries:   6,
		RetryWait: 10 * time.Second,
	}, nil
}

func (e EthAPI) GetProof(address string, storageKeys []string, blockHex string) (EthGetProofResponse, error) {
	return retry(e.Retries, e.RetryWait, func() (EthGetProofResponse, error) {
		var proofResponse EthGetProofResponse
		if err := e.client.Client().Call(&proofResponse, "eth_getProof", address, storageKeys, blockHex); err != nil {
			return EthGetProofResponse{}, err
		}

		return proofResponse, nil
	})
}

func (e EthAPI) GetBlockNumber() (string, int64, error) {
	var blockNumberHex string
	if err := e.client.Client().Call(&blockNumberHex, "eth_blockNumber"); err != nil {
		return "", 0, err
	}

	blockNumber, err := strconv.ParseInt(blockNumberHex, 0, 0)
	if err != nil {
		return "", 0, err
	}

	return blockNumberHex, blockNumber, nil
}
