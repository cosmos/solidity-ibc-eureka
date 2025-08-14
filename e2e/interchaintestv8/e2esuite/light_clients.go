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

// StoreEthereumLightClient stores the Ethereum light client on the given Cosmos chain and returns the hex-encoded checksum of the light client.
func (s *TestSuite) StoreEthereumLightClient(ctx context.Context, cosmosChain *cosmos.CosmosChain, simdRelayerUser ibc.Wallet) string {
	wasmBinary := s.getWasmLightClientBinary()
	checksum := s.PushNewWasmClientProposal(ctx, cosmosChain, simdRelayerUser, wasmBinary)
	s.Require().NotEmpty(checksum, "checksum was empty but should not have been")

	s.T().Logf("Stored Ethereum light client with checksum %s", checksum)

	return checksum
}

// StoreAttestorLightClient stores the Attestor light client on the given Cosmos chain and returns the hex-encoded checksum of the light client.
func (s *TestSuite) StoreAttestorLightClient(ctx context.Context, cosmosChain *cosmos.CosmosChain, simdRelayerUser ibc.Wallet) string {
	wasmBinary := s.getWasmAttestorLightClientBinary()
	checksum := s.PushNewWasmClientProposal(ctx, cosmosChain, simdRelayerUser, wasmBinary)
	s.Require().NotEmpty(checksum, "checksum was empty but should not have been")

	s.T().Logf("Stored Attestor light client with checksum %s", checksum)

	return checksum
}

// ClientType represents the type of light client
type ClientType string

const (
	EthereumClient ClientType = "ethereum"
	AttestorClient ClientType = "attestor"
)

func (s *TestSuite) getWasmLightClientBinary() *os.File {
	return s.getWasmLightClientBinaryByType(EthereumClient)
}

func (s *TestSuite) getWasmAttestorLightClientBinary() *os.File {
	return s.getWasmLightClientBinaryByType(AttestorClient)
}

func (s *TestSuite) getWasmLightClientBinaryByType(clientType ClientType) *os.File {
	switch clientType {
	case EthereumClient:
		return s.getEthereumLightClientBinary()
	case AttestorClient:
		return s.getAttestorLightClientBinary()
	default:
		s.T().Fatalf("Unknown client type: %s", clientType)
		return nil
	}
}

func (s *TestSuite) getEthereumLightClientBinary() *os.File {
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
		s.T().Log("Using local Wasm Ethereum light client binary")
		file, err := wasm.GetLocalWasmEthLightClient()
		s.Require().NoError(err, "Failed to get local Wasm Ethereum light client binary")
		return file
	}

	// Otherwise, we download the Wasm light client binary from the GitHub release of the given tag
	s.T().Logf("Downloading Wasm Ethereum light client binary for tag %s", s.WasmLightClientTag)
	file, err := wasm.DownloadWasmEthLightClientRelease(wasm.Release{
		TagName: s.WasmLightClientTag,
	})
	s.Require().NoError(err, "Failed to download Wasm Ethereum light client binary from release")
	return file
}

func (s *TestSuite) getAttestorLightClientBinary() *os.File {
	// If it is empty or set to "local", we use the local Wasm light client binary
	if s.WasmAttestorLightClientTag == "" || s.WasmAttestorLightClientTag == testvalues.EnvValueWasmLightClientTag_Local {
		s.T().Log("Using local Wasm Attestor light client binary")
		file, err := wasm.GetLocalWasmAttestorLightClient()
		s.Require().NoError(err, "Failed to get local Wasm Attestor light client binary")
		return file
	}

	// Otherwise, we download the Wasm light client binary from the GitHub release of the given tag
	s.T().Logf("Downloading Wasm Attestor light client binary for tag %s", s.WasmAttestorLightClientTag)
	file, err := wasm.DownloadWasmAttestorLightClientRelease(wasm.Release{
		TagName: s.WasmAttestorLightClientTag,
	})
	s.Require().NoError(err, "Failed to download Wasm Attestor light client binary from release")
	return file
}
