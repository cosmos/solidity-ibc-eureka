package e2esuite

import (
	"context"
	"os"
	"slices"

	"github.com/strangelove-ventures/interchaintest/v8/chain/cosmos"
	"github.com/strangelove-ventures/interchaintest/v8/ibc"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/wasm"
)

// StoreLightClient stores the Ethereum light client on the given Cosmos chain and returns the hex-encoded checksum of the light client.
func (s *TestSuite) StoreLightClient(ctx context.Context, cosmosChain *cosmos.CosmosChain, simdRelayerUser ibc.Wallet) string {
	wasmBinary := s.getWasmLightClientBinary()
	checksum := s.PushNewWasmClientProposal(ctx, cosmosChain, simdRelayerUser, wasmBinary)
	s.Require().NotEmpty(checksum, "checksum was empty but should not have been")

	s.T().Logf("Stored wasm light client with checksum %s", checksum)

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
	allNonPowEvmTestnets := []string{testvalues.EthTestnetTypePoS, testvalues.EthTestnetTypeArbitrum, testvalues.EthTestnetTypeOptimism}

	s.Require().True(slices.Contains(allNonPowEvmTestnets, s.ethTestnetType))

	// If it is empty or set to "local", we use the local Wasm light client binary
	if s.WasmLightClientTag == "" || s.WasmLightClientTag == testvalues.EnvValueWasmLightClientTag_Local {
		switch s.ethTestnetType {
		case testvalues.EthTestnetTypeArbitrum, testvalues.EthTestnetTypeOptimism:
			s.T().Log("Using local Wasm attestor light client binary")
			file, err := wasm.GetLocalWasmAttestorLightClient()
			s.Require().NoError(err, "Failed to get local Wasm attestor light client binary")
			return file
		default:
			s.T().Log("Using local Wasm Ethereum light client binary")
			file, err := wasm.GetLocalWasmEthLightClient()
			s.Require().NoError(err, "Failed to get local Wasm Ethereum light client binary")
			return file
		}
	}

	// Otherwise, we download the Wasm light client binary from the GitHub release of the given tag
	s.T().Logf("Downloading Wasm light client binary for tag %s", s.WasmLightClientTag)
	file, err := wasm.DownloadWasmLightClientRelease(wasm.Release{
		TagName: s.WasmLightClientTag,
	})
	s.Require().NoError(err, "Failed to download Wasm Ethereum light client binary from release")
	return file
}
