package e2esuite

import (
	"context"
	"fmt"
	"io"
	"net/http"
	"os"
	"path/filepath"

	"github.com/strangelove-ventures/interchaintest/v8/chain/cosmos"
	"github.com/strangelove-ventures/interchaintest/v8/ibc"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
)

const (
	lightClientFileName  = "cw_ics08_wasm_eth.wasm.gz"
	localLightClientPath = "e2e/interchaintestv8/wasm/" + lightClientFileName
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
	var (
		file            *os.File
		err             error
		lightClientUsed string
	)
	if s.EthLightClientTag == "" {
		lightClientUsed = localLightClientPath
		s.T().Logf("Using local light client %s", localLightClientPath)

		file, err = os.Open(localLightClientPath)
		s.Require().NoError(err)
	} else {
		downloadUrl := fmt.Sprintf("https://github.com/cosmos/solidity-ibc-eureka/releases/download/%s/%s", s.EthLightClientTag, lightClientFileName)
		lightClientUsed = downloadUrl
		s.T().Logf("Downloading light client from %s", downloadUrl)

		resp, err := http.Get(downloadUrl) //nolint:gosec
		s.Require().NoError(err)
		defer resp.Body.Close()

		tmpDownloadDir := s.T().TempDir()
		filePath := filepath.Join(tmpDownloadDir, lightClientFileName)
		out, err := os.Create(filePath)
		s.Require().NoError(err)

		_, err = io.Copy(out, resp.Body)
		s.Require().NoError(err)
		out.Close()

		file, err = os.Open(filePath)
		s.Require().NoError(err)
	}
	defer file.Close()

	etheruemClientChecksum := s.PushNewWasmClientProposal(ctx, cosmosChain, simdRelayerUser, file)
	s.Require().NotEmpty(etheruemClientChecksum, "checksum was empty but should not have been")

	s.T().Logf("Stored Etheruem light client from %s with checksum %s", lightClientUsed, etheruemClientChecksum)

	return etheruemClientChecksum
}

func (s *TestSuite) createDummyLightClient(ctx context.Context, cosmosChain *cosmos.CosmosChain, simdRelayerUser ibc.Wallet) string {
	file, err := os.Open("e2e/interchaintestv8/wasm/wasm_dummy_light_client.wasm.gz")
	s.Require().NoError(err)

	dummyClientChecksum := s.PushNewWasmClientProposal(ctx, cosmosChain, simdRelayerUser, file)
	s.Require().NotEmpty(dummyClientChecksum, "checksum was empty but should not have been")

	return dummyClientChecksum
}
