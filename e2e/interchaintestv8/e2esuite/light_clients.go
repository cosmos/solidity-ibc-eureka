package e2esuite

import (
	"context"
	"os"

	"github.com/strangelove-ventures/interchaintest/v8/chain/cosmos"
	"github.com/strangelove-ventures/interchaintest/v8/ibc"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/wasm"
)

// StoreEthereumLightClient stores the Ethereum light client on the given Cosmos chain and returns the hex-encoded checksum of the light client.
func (s *TestSuite) StoreEthereumLightClient(ctx context.Context, cosmosChain *cosmos.CosmosChain, simdRelayerUser ibc.Wallet) string {
	wasmBinary := s.getWasmLightClientBinary()
	checksum := s.PushNewWasmClientProposal(ctx, cosmosChain, simdRelayerUser, wasmBinary)
	s.Require().NotEmpty(checksum, "checksum was empty but should not have been")

	s.T().Logf("Stored Ethereum light client with checksum %s", checksum)

	return checksum
}

func (s *TestSuite) getWasmLightClientBinary() *os.File {
	// For PoW testnets, we use the dummy light client
	if s.ethTestnetType == testvalues.EthTestnetTypePoW {
		s.T().Log("Using dummy Wasm light client for PoW testnet")
		file, err := wasm.GetWasmDummyLightClient()
		s.Require().NoError(err, "Failed to get local Wasm light client binary")
		return file
	}

	s.Require().Equal(s.ethTestnetType, testvalues.EthTestnetTypePoS, "Invalid Ethereum testnet type")

	// If it is empty or set to "local", we use the local Wasm light client binary
	if s.WasmLightClientTag == "" || s.WasmLightClientTag == testvalues.EnvValueWasmLightClientTag_Local {
		s.T().Log("Using local Wasm light client binary")
		file, err := wasm.GetLocalWasmEthLightClient()
		s.Require().NoError(err, "Failed to get local Wasm light client binary")
		return file
	}

	// Otherwise, we download the Wasm light client binary from the GitHub release of the given tag
	s.T().Logf("Downloading Wasm light client binary for tag %s", s.WasmLightClientTag)
	file, err := wasm.DownloadWasmEthLightClientRelease(wasm.Release{
		TagName: s.WasmLightClientTag,
	})
	s.Require().NoError(err, "Failed to download Wasm light client binary from release")
	return file
}
