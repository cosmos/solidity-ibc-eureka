package main

import (
	"context"
	"crypto/ecdsa"
	"os"
	"testing"

	"github.com/cosmos/interchaintest/v10/ibc"
	"github.com/stretchr/testify/require"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/attestor"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/e2esuite"
	tv "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
	attestortypes "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/attestor"
)

// TestCosmosToEVMAttestor E2E test that exercises Cosmos state attestation on EVM chain using
// a light client in Solidity
func TestCosmosToEVMAttestor(t *testing.T) {
	ts := newCosmosToEVMAttestorTestSuite(t)

	t.Run("StateAttestation", func(t *testing.T) {
		// ARRANGE
		// Given Cosmos chain height
		const height = uint64(1)

		// ACT
		resp, err := attestor.GetStateAttestation(ts.Ctx, ts.attestorClient, height)

		// ASSERT
		require.NoError(t, err, "unable to get state attestation")

		// Then the signature is not empty
		sig := resp.GetAttestation().GetSignature()
		require.NotEmpty(t, sig, "signature is empty")

		t.Logf("State attestation signature: 0x%x", sig)
	})
}

type cosmosToEVMAttestorTestSuite struct {
	*testing.T

	Ctx context.Context

	base *e2esuite.TestSuite

	// users
	evmDeployer  *ecdsa.PrivateKey
	cosmosSender ibc.Wallet

	// clients
	attestorClient attestortypes.AttestationServiceClient
}

func newCosmosToEVMAttestorTestSuite(t *testing.T) *cosmosToEVMAttestorTestSuite {
	t.Helper()

	ctx := context.Background()

	// 1. Ensure some ENV
	var (
		envEthWasmType    = tv.EnvEnsure(tv.EnvKeyE2EEthWasmType, tv.EthWasmTypeAttestor)
		envEthTestnetType = tv.EnvEnsure(tv.EnvKeyEthTestnetType, tv.EthTestnetTypeOptimism)
		_                 = tv.EnvEnsure(tv.EnvKeyRustLog, tv.EnvValueRustLog_Info)
	)

	// Skip test if not relevant
	if envEthWasmType != tv.EthWasmTypeAttestor {
		t.Skipf(
			"Skipping: expecting %s to be %q, got %q",
			tv.EnvKeyE2EEthWasmType,
			tv.EthWasmTypeAttestor,
			envEthWasmType,
		)
	}

	if envEthTestnetType != tv.EthTestnetTypeOptimism {
		t.Skipf(
			"Skipping: expecting %s to be %q, got %q",
			tv.EnvKeyEthTestnetType,
			tv.EthTestnetTypeOptimism,
			envEthTestnetType,
		)
	}

	// 2. Setup base test suite as the current E2E framework relies on it
	base := &e2esuite.TestSuite{
		EthWasmType:        envEthWasmType,
		WasmLightClientTag: "",
	}

	// This should provision two chains in docker (cosmos `simd` node and EVM node base on Optimism)
	base.SetT(t)
	base.SetupSuite(ctx)

	// Set some ENV related to RPC
	os.Setenv(tv.EnvKeyEthRPC, base.EthChain.RPC)
	os.Setenv(tv.EnvKeyTendermintRPC, base.CosmosChains[0].GetHostRPCAddress())

	// 3. Provision users
	evmDeployer, err := base.EthChain.CreateAndFundUser()
	require.NoError(t, err, "unable to provision EVM deployer")

	cosmosSender := base.CreateAndFundCosmosUser(ctx, base.CosmosChains[0])

	// 4. Setup ONE cosmos attestor (for the sake of the current test)
	// TODO: support for arbitrary number of attestors in the future with
	// TODO: private keys provisioning on the fly
	const attestorPort = 9000

	attestorAddr := runAttestor(t, attestor.CosmosBinary, func(c *attestor.AttestorConfig) string {
		c.Server.Port = attestorPort
		c.Cosmos.URL = base.CosmosChains[0].GetHostRPCAddress()

		return "/tmp/attestor_0.toml"
	})

	attestorClient, err := attestor.GetAttestationServiceClient(attestorAddr)
	require.NoError(t, err, "unable to get attestation service client")

	// todo setup stuff
	//    todo deploy cosmos LC on EVM
	//    todo run a relayer
	return &cosmosToEVMAttestorTestSuite{
		T:    t,
		Ctx:  ctx,
		base: base,

		evmDeployer:  evmDeployer,
		cosmosSender: cosmosSender,

		attestorClient: attestorClient,
	}
}

// runAttestor spins up a separate process that runs the attestor binary
// based on the given config.
func runAttestor(
	t *testing.T,
	attestorType attestor.AttestorBinaryPath,
	withConfig func(c *attestor.AttestorConfig) (configPath string),
) (serverAddr string) {
	t.Helper()

	config := attestor.DefaultAttestorConfig()
	configPath := withConfig(config)

	err := config.WriteTomlConfig(configPath)
	require.NoError(t, err, "unable to write attestor config")

	proc, err := attestor.StartAttestor(configPath, attestorType)
	require.NoError(t, err, "unable to start attestor")

	t.Cleanup(func() {
		err = proc.Kill()
		require.NoError(t, err, "unable to kill attestor process")

		err := attestor.CleanupConfig(configPath)
		require.NoError(t, err, "unable to cleanup attestor config")
	})

	return config.GetServerAddress()
}
