package solana

import (
	"github.com/gagliardetto/solana-go/rpc"
)

type Solana struct {
	RPCClient *rpc.Client
}

func NewSolana(rpcURL string) (Solana, error) {
	return Solana{
		RPCClient: rpc.New(rpcURL),
	}, nil
}

func NewLocalnetSolana() (Solana, error) {
	return NewSolana(rpc.LocalNet.RPC)
}
