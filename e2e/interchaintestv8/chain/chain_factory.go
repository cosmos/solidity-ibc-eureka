package chain

import (
	"fmt"
	"strings"

	"go.uber.org/zap"

	"github.com/cosmos/interchaintest/v10"
	"github.com/cosmos/interchaintest/v10/ibc"
)

// ExtendedChainFactory extends the built-in chain factory with Solana support
type ExtendedChainFactory struct {
	log   *zap.Logger
	specs []*interchaintest.ChainSpec
}

// NewExtendedChainFactory creates a new factory that supports Solana chains
func NewExtendedChainFactory(log *zap.Logger, specs []*interchaintest.ChainSpec) *ExtendedChainFactory {
	return &ExtendedChainFactory{log: log, specs: specs}
}

func (f *ExtendedChainFactory) Count() int {
	return len(f.specs)
}

func (f *ExtendedChainFactory) Chains(testName string) ([]ibc.Chain, error) {
	chains := make([]ibc.Chain, len(f.specs))
	for i, s := range f.specs {
		// For Solana chains, use the ChainConfig directly without calling Config()
		var cfg *ibc.ChainConfig
		if s.Type == Solana {
			// Use the ChainConfig directly for Solana
			cfg = &s.ChainConfig
		} else {
			// For other chains, use the normal Config() method
			c, err := s.Config(f.log)
			if err != nil {
				// Prefer to wrap the error with the chain name if possible.
				if s.Name != "" {
					return nil, fmt.Errorf("failed to build chain config %s: %w", s.Name, err)
				}
				return nil, fmt.Errorf("failed to build chain config at index %d: %w", i, err)
			}
			cfg = c
		}

		// Use our extended BuildChain that supports Solana
		chain, err := BuildChain(f.log, testName, *cfg, s.NumValidators, s.NumFullNodes)
		if err != nil {
			return nil, err
		}
		chains[i] = chain
	}

	return chains, nil
}

func (f *ExtendedChainFactory) Name() string {
	parts := make([]string, len(f.specs))
	for i, s := range f.specs {
		// Ignoring error here because if we fail to generate the config,
		// another part of the factory stack should have failed properly before we got here.
		cfg, _ := s.Config(f.log)

		v := s.Version
		if v == "" && cfg != nil && len(cfg.Images) > 0 {
			v = cfg.Images[0].Version
		}

		name := "unknown"
		if cfg != nil {
			name = cfg.Name
		}

		parts[i] = name + "@" + v
	}
	return strings.Join(parts, "+")
}
