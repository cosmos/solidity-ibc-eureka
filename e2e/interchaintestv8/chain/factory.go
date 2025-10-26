package chain

import (
	"fmt"

	"go.uber.org/zap"

	"github.com/cosmos/interchaintest/v10/chain/cosmos"
	"github.com/cosmos/interchaintest/v10/chain/ethereum/foundry"
	"github.com/cosmos/interchaintest/v10/chain/ethereum/geth"
	"github.com/cosmos/interchaintest/v10/ibc"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/chain/solana"
)

// Chain type constant for Solana
const Solana = "solana"

// BuildChain builds a chain instance based on the provided configuration
// This extends the default interchaintest chain factory with Solana support
func BuildChain(log *zap.Logger, testName string, cfg ibc.ChainConfig, numValidators, numFullNodes *int) (ibc.Chain, error) {
	switch cfg.Type {
	case ibc.Cosmos:
		nv := 2
		if numValidators != nil {
			nv = *numValidators
		}
		nf := 1
		if numFullNodes != nil {
			nf = *numFullNodes
		}
		return cosmos.NewCosmosChain(testName, cfg, nv, nf, log), nil

	case ibc.Ethereum:
		switch cfg.Bin {
		case "anvil":
			return foundry.NewAnvilChain(testName, cfg, log), nil
		case "geth":
			return geth.NewGethChain(testName, cfg, log), nil
		default:
			return nil, fmt.Errorf("unknown binary: %s for ethereum chain type, must be anvil or geth", cfg.Bin)
		}

	case Solana:
		return solana.NewSolanaChain(testName, cfg, log), nil

	default:
		return nil, fmt.Errorf("unexpected error, unknown chain type: %s for chain: %s", cfg.Type, cfg.Name)
	}
}