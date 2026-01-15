package e2esuite

import (
	"context"

	"github.com/cosmos/interchaintest/v10/chain/cosmos"
	"github.com/cosmos/interchaintest/v10/ibc"
)

func (s *TestSuite) setupCosmosChains(ctx context.Context, chains []ibc.Chain) {
	if len(chains) == 0 {
		return
	}

	for _, chain := range chains {
		cosmosChain := chain.(*cosmos.CosmosChain)
		s.Cosmos.Chains = append(s.Cosmos.Chains, cosmosChain)
	}

	// map all query request types to their gRPC method paths for cosmos chains
	s.Require().NoError(populateQueryReqToPath(ctx, s.Cosmos.Chains[0]))

	// Fund user accounts
	for _, chain := range chains {
		s.Cosmos.Users = append(s.Cosmos.Users, s.CreateAndFundCosmosUser(ctx, chain.(*cosmos.CosmosChain)))
	}

	s.Cosmos.proposalIDs = make(map[string]uint64)
	for _, chain := range s.Cosmos.Chains {
		s.Cosmos.proposalIDs[chain.Config().ChainID] = 1
	}
}
