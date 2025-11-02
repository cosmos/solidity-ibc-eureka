package e2esuite

import (
	"context"
	"fmt"
	"os"

	"github.com/cosmos/interchaintest/v10/chain/cosmos"
	"github.com/cosmos/interchaintest/v10/ibc"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
)

// StoreEthereumLightClient stores the Ethereum light client on the given Cosmos chain and returns the hex-encoded checksum of the light client.
func (s *TestSuite) StoreEthereumLightClient(ctx context.Context, cosmosChain *cosmos.CosmosChain, simdRelayerUser ibc.Wallet) string {
	switch s.ethTestnetType {
	case testvalues.EthTestnetTypePoW:
		return s.createDummyLightClient(ctx, cosmosChain, simdRelayerUser)
	case testvalues.EthTestnetTypePoS:
		return s.storeEthereumLightClient(ctx, cosmosChain, simdRelayerUser)
	default:
		panic(fmt.Sprintf("Unrecognized Ethereum testnet type: %v", s.ethTestnetType))
	}
}

func (s *TestSuite) storeEthereumLightClient(
	ctx context.Context,
	cosmosChain *cosmos.CosmosChain,
	simdRelayerUser ibc.Wallet,
) string {
	file, err := os.Open("e2e/interchaintestv8/wasm/cw_ics08_wasm_eth.wasm.gz")
	s.Require().NoError(err)

	etheruemClientChecksum := s.PushNewWasmClientProposal(ctx, cosmosChain, simdRelayerUser, file)
	s.Require().NotEmpty(etheruemClientChecksum, "checksum was empty but should not have been")

	return etheruemClientChecksum
}

func (s *TestSuite) createDummyLightClient(ctx context.Context, cosmosChain *cosmos.CosmosChain, simdRelayerUser ibc.Wallet) string {
	file, err := os.Open("e2e/interchaintestv8/wasm/wasm_dummy_light_client.wasm.gz")
	s.Require().NoError(err)

	dummyClientChecksum := s.PushNewWasmClientProposal(ctx, cosmosChain, simdRelayerUser, file)
	s.Require().NotEmpty(dummyClientChecksum, "checksum was empty but should not have been")

	return dummyClientChecksum
}
