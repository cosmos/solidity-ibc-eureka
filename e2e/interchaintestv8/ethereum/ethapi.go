package ethereum

import (
	"time"

	"github.com/ethereum/go-ethereum/ethclient"
)

type EthAPI struct {
	Client *ethclient.Client

	Retries   int
	RetryWait time.Duration
}

func NewEthAPI(rpc string) (EthAPI, error) {
	ethClient, err := ethclient.Dial(rpc)
	if err != nil {
		return EthAPI{}, err
	}

	return EthAPI{
		Client:    ethClient,
		Retries:   6,
		RetryWait: 10 * time.Second,
	}, nil
}
