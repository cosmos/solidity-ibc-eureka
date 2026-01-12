package e2esuite

import (
	"context"
	"os"
	"slices"

	"github.com/cosmos/interchaintest/v10/chain/cosmos"
	"github.com/cosmos/interchaintest/v10/ibc"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/wasm"
)

// StoreLightClient stores the light client on the given Cosmos chain and returns the hex-encoded checksum of the light client.
// For native attestor (attestor-native), returns empty string as no wasm binary is needed.
func (s *TestSuite) StoreLightClient(ctx context.Context, cosmosChain *cosmos.CosmosChain, simdRelayerUser ibc.Wallet) string {
	// Native attestor doesn't need a wasm binary
	if s.EthWasmType == testvalues.EthWasmTypeAttestorNative {
		s.T().Log("Using native attestor - no wasm storage needed")
		return ""
	}

	wasmBinary := s.getWasmLightClientBinary()
	if wasmBinary == nil {
		return ""
	}
	checksum := s.PushNewWasmClientProposal(ctx, cosmosChain, simdRelayerUser, wasmBinary)
	s.Require().NotEmpty(checksum, "checksum was empty but should not have been")

	s.T().Logf("Stored wasm light client with checksum %s", checksum)

	return checksum
}

// StoreSolanaLightClient stores the Solana light client on the given Cosmos chain and returns the hex-encoded checksum of the light client.
func (s *TestSuite) StoreSolanaLightClient(ctx context.Context, cosmosChain *cosmos.CosmosChain, simdRelayerUser ibc.Wallet) string {
	// For Solana verification, we use the dummy light client for testing
	s.T().Log("Using dummy Wasm light client for Solana verification")
	wasmBinary, err := wasm.GetWasmDummyLightClient()
	s.Require().NoError(err, "Failed to get dummy Wasm light client binary")

	checksum := s.PushNewWasmClientProposal(ctx, cosmosChain, simdRelayerUser, wasmBinary)
	s.Require().NotEmpty(checksum, "checksum was empty but should not have been")

	s.T().Logf("Stored Solana light client with checksum %s", checksum)

	return checksum
}

// StoreSolanaAttestedLightClient stores the attestor-based Solana light client on the given Cosmos chain
// and returns the hex-encoded checksum of the light client.
func (s *TestSuite) StoreSolanaAttestedLightClient(ctx context.Context, cosmosChain *cosmos.CosmosChain, simdRelayerUser ibc.Wallet) string {
	s.T().Log("Using attestor Wasm light client for Solana verification")
	wasmBinary, err := wasm.GetLocalWasmAttestationLightClient()
	s.Require().NoError(err, "Failed to get attestor Wasm light client binary")

	checksum := s.PushNewWasmClientProposal(ctx, cosmosChain, simdRelayerUser, wasmBinary)
	s.Require().NotEmpty(checksum, "checksum was empty but should not have been")

	s.T().Logf("Stored Solana attested light client with checksum %s", checksum)

	return checksum
}

func (s *TestSuite) getWasmLightClientBinary() *os.File {
	// For PoW testnets, we use the dummy light client
	if s.ethTestnetType == testvalues.EthTestnetTypePoW && s.EthWasmType == testvalues.EthWasmTypeDummy {
		s.T().Log("Using dummy Wasm light client for PoW testnet")
		file, err := wasm.GetWasmDummyLightClient()
		s.Require().NoError(err, "Failed to get local Wasm light client binary")
		return file
	}
	// For PoW testnets using wasm attestor
	if s.ethTestnetType == testvalues.EthTestnetTypePoW && s.EthWasmType == testvalues.EthWasmTypeAttestorWasm {
		s.T().Log("Using attestor Wasm light client for PoW testnet")
		file, err := wasm.GetLocalWasmAttestationLightClient()
		s.Require().NoError(err, "Failed to get local Wasm light client binary")
		return file
	}
	// For native attestor, no wasm binary is needed
	if s.EthWasmType == testvalues.EthWasmTypeAttestorNative {
		s.T().Log("Using native attestor - no wasm binary needed")
		return nil
	}

	allNonPowEvmTestnets := []string{testvalues.EthTestnetTypePoS, testvalues.EthTestnetTypeArbitrum, testvalues.EthTestnetTypeOptimism}
	s.Require().True(slices.Contains(allNonPowEvmTestnets, s.ethTestnetType))

	acceptedWasmKinds := []string{testvalues.EthWasmTypeFull, testvalues.EthWasmTypeAttestorWasm}
	s.Require().True(slices.Contains(acceptedWasmKinds, s.EthWasmType))

	// If it is empty or set to "local", we use the local Wasm light client binary
	// NOTE: We ignore the wasm kind here because we don't care about
	// testing attestors on PoS and L2s only support attested solutions
	if s.WasmLightClientTag == "" || s.WasmLightClientTag == testvalues.EnvValueWasmLightClientTag_Local {
		switch s.ethTestnetType {
		case testvalues.EthTestnetTypeArbitrum, testvalues.EthTestnetTypeOptimism:
			s.T().Log("Using local Wasm attestor light client binary")
			file, err := wasm.GetLocalWasmAttestationLightClient()
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
	s.Require().NoError(err, "Failed to download Wasm light client binary from release")
	return file
}
