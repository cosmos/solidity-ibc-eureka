package main

import (
	"context"
	"crypto/ecdsa"
	"os"
	"testing"

	"github.com/cosmos/interchaintest/v10/ibc"
	"github.com/stretchr/testify/require"

	ethcommon "github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/ethclient"

	"github.com/cosmos/solidity-ibc-eureka/packages/go-abigen/ics20transfer"
	"github.com/cosmos/solidity-ibc-eureka/packages/go-abigen/ics26router"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/attestor"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/e2esuite"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/ethereum"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/relayer"
	tv "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
	attestortypes "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/attestor"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/erc20"
	relayertypes "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/relayer"
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
	relayerClient  relayertypes.RelayerServiceClient

	// evmContracts
	evmContracts evmContracts
}

func newCosmosToEVMAttestorTestSuite(t *testing.T) *cosmosToEVMAttestorTestSuite {
	t.Helper()

	// note: this is really bad, but other tests internals expect chdir
	require.NoError(t, os.Chdir("../.."))

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

	var (
		evmChain    = base.EthChain
		cosmosChain = base.CosmosChains[0]
	)

	// Set some ENV related to RPC
	os.Setenv(tv.EnvKeyEthRPC, evmChain.RPC)
	os.Setenv(tv.EnvKeyTendermintRPC, cosmosChain.GetHostRPCAddress())

	// 3. Provision users
	evmDeployer, err := evmChain.CreateAndFundUser()
	require.NoError(t, err, "unable to provision EVM deployer")

	cosmosSender := base.CreateAndFundCosmosUser(ctx, cosmosChain)

	// 4. Setup ONE cosmos attestor (for the sake of the current test)
	// TODO: support for arbitrary number of attestors in the future with
	// TODO: private keys provisioning on the fly
	const attestorPort = 9000

	attestorAddr := runAttestor(t, attestor.CosmosBinary, func(c *attestor.AttestorConfig) string {
		c.Server.Port = attestorPort
		c.Cosmos.URL = cosmosChain.GetHostRPCAddress()

		return "/tmp/attestor_0.toml"
	})

	attestorClient, err := attestor.GetAttestationServiceClient(attestorAddr)
	require.NoError(t, err, "unable to get attestation service client")

	// 4. Deploy IBC contracts
	out, err := base.EthChain.ForgeScript(evmDeployer, tv.E2EDeployScriptPath)
	require.NoError(t, err, "unable to deploy ibc contracts")

	evmContractWrappers := extractContractWrappers(t, out, base.EthChain.RPCClient)

	// 5. Start the relayer
	relayerClient := runRelayer(t, relayer.NewConfig([]relayer.ModuleConfig{
		{
			Name:     relayer.ModuleCosmosToEthAttested,
			SrcChain: cosmosChain.Config().ChainID,
			DstChain: evmChain.ChainID.String(),
			Config: relayer.CosmosToEthAttestedModuleConfig{
				AttestedChainId:  cosmosChain.Config().ChainID,
				AggregatorConfig: relayer.DefaultAggregatorConfig(),
				AttestedRpcUrl:   cosmosChain.GetHostRPCAddress(),
				Ics26Address:     evmContractWrappers.ICS26RouterAddress.String(),
				EthRpcUrl:        evmChain.RPC,
			},
		},
	}))

	// todo setup stuff
	//    todo deploy cosmos LC on EVM
	return &cosmosToEVMAttestorTestSuite{
		T:    t,
		Ctx:  ctx,
		base: base,

		evmDeployer:  evmDeployer,
		cosmosSender: cosmosSender,

		attestorClient: attestorClient,
		relayerClient:  relayerClient,

		evmContracts: evmContractWrappers,
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

func runRelayer(t *testing.T, config relayer.Config) relayertypes.RelayerServiceClient {
	t.Helper()

	err := config.GenerateConfigFile(tv.RelayerConfigFilePath)
	require.NoError(t, err, "unable to generate relayer config file")

	proc, err := relayer.StartRelayer(tv.RelayerConfigFilePath)
	require.NoError(t, err, "unable to start relayer")

	t.Cleanup(func() {
		os.Remove(tv.RelayerConfigFilePath)
		if err := proc.Kill(); err != nil {
			t.Logf("unable to kill relayer process: %v", err)
		}
	})

	client, err := relayer.GetGRPCClient(relayer.DefaultRelayerGRPCAddress())
	require.NoError(t, err, "unable to get relayer client")

	return client
}

type evmContracts struct {
	ICS26RouterAddress ethcommon.Address
	ICS26Router        *ics26router.Contract

	ICS20TransferAddress ethcommon.Address
	ICS20Transfer        *ics20transfer.Contract

	ERC20Address ethcommon.Address
	ERC20        *erc20.Contract
}

func extractContractWrappers(t *testing.T, raw []byte, evmClient *ethclient.Client) evmContracts {
	t.Helper()

	addresses, err := ethereum.GetEthContractsFromDeployOutput(string(raw))
	require.NoError(t, err, "unable to parse eth contracts from deploy output")

	ics26RouterAddress := ethcommon.HexToAddress(addresses.Ics26Router)
	ics26Contract, err := ics26router.NewContract(ics26RouterAddress, evmClient)
	require.NoError(t, err, "unable to create ics26 wrapper")

	ics20TransferAddress := ethcommon.HexToAddress(addresses.Ics20Transfer)
	ics20Contract, err := ics20transfer.NewContract(ics20TransferAddress, evmClient)
	require.NoError(t, err, "unable to create ics20 wrapper")

	erc20Address := ethcommon.HexToAddress(addresses.Erc20)
	erc20Contract, err := erc20.NewContract(erc20Address, evmClient)
	require.NoError(t, err, "unable to create erc20 wrapper")

	return evmContracts{
		ICS26RouterAddress: ics26RouterAddress,
		ICS26Router:        ics26Contract,

		ICS20TransferAddress: ics20TransferAddress,
		ICS20Transfer:        ics20Contract,

		ERC20Address: erc20Address,
		ERC20:        erc20Contract,
	}
}
