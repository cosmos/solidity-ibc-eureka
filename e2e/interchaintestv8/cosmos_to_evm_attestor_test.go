package main

import (
	"context"
	"crypto/ecdsa"
	"encoding/hex"
	"fmt"
	"math/big"
	"os"
	"strconv"
	"strings"
	"testing"
	"time"

	"github.com/stretchr/testify/require"

	ethcommon "github.com/ethereum/go-ethereum/common"
	ethtypes "github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/crypto"
	"github.com/ethereum/go-ethereum/ethclient"

	sdkmath "cosmossdk.io/math"

	sdk "github.com/cosmos/cosmos-sdk/types"
	banktypes "github.com/cosmos/cosmos-sdk/x/bank/types"

	transfertypes "github.com/cosmos/ibc-go/v10/modules/apps/transfer/types"
	clienttypes "github.com/cosmos/ibc-go/v10/modules/core/02-client/types"
	clienttypesv2 "github.com/cosmos/ibc-go/v10/modules/core/02-client/v2/types"
	channeltypesv2 "github.com/cosmos/ibc-go/v10/modules/core/04-channel/v2/types"

	"github.com/cosmos/interchaintest/v10/chain/cosmos"
	"github.com/cosmos/interchaintest/v10/ibc"

	"github.com/cosmos/solidity-ibc-eureka/packages/go-abigen/attestation"
	"github.com/cosmos/solidity-ibc-eureka/packages/go-abigen/ibcerc20"
	"github.com/cosmos/solidity-ibc-eureka/packages/go-abigen/ics20transfer"
	"github.com/cosmos/solidity-ibc-eureka/packages/go-abigen/ics26router"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/attestor"
	cosmosutils "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/cosmos"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/e2esuite"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/ethereum"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/relayer"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
	attestortypes "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/attestor"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/erc20"
	relayertypes "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/relayer"
)

// TestCosmosToEVMAttestor E2E test that exercises Cosmos state attestation on EVM chain using
// a light client in Solidity.
//
// TODO: Decide to either refactor the other tests to a similar setup, or refactor this to follow the existing pattern.
func TestCosmosToEVMAttestor(t *testing.T) {
	ts := newCosmosToEVMAttestorTestSuite(t)

	t.Run("StateAttestation", func(t *testing.T) {
		// ARRANGE
		// Given Cosmos chain height
		const height = uint64(1)

		// ACT
		resp, err := attestor.GetStateAttestation(ts.ctx, ts.attestorClient, height)

		// ASSERT
		require.NoError(t, err, "unable to get state attestation")

		// Then the signature is not empty
		sig := resp.GetAttestation().GetSignature()
		require.NotEmpty(t, sig, "signature is empty")

		t.Logf("State attestation signature: 0x%x", sig)
	})

	// Transfers ICS20 token from Cosmos to EVM
	t.Run("ICS20Transfer", func(t *testing.T) {
		var (
			ctx = ts.ctx

			evmChain    = ts.base.EthChains[0]
			cosmosChain = ts.base.CosmosChains[0]

			evmDeployerAddr = crypto.PubkeyToAddress(ts.evmDeployer.PublicKey)

			transferAmount = big.NewInt(testvalues.TransferAmount)
			transferCoin   = sdk.NewCoin(cosmosChain.Config().Denom, sdkmath.NewIntFromBigInt(transferAmount))

			packetTimeout = uint64(time.Now().Add(30 * time.Minute).Unix())
		)

		var cosmosSendTxHash []byte

		ts.do("1: Send transfer on Cosmos", func() {
			// prepare packet with payload with transfer action :)
			transferPayload := transfertypes.FungibleTokenPacketData{
				Denom:    transferCoin.Denom,
				Amount:   transferCoin.Amount.String(),
				Sender:   ts.cosmosDeployer.FormattedAddress(),
				Receiver: strings.ToLower(evmDeployerAddr.Hex()),
				Memo:     "nativesend",
			}

			msgSendPacket := channeltypesv2.MsgSendPacket{
				SourceClient:     testvalues.FirstWasmClientID,
				TimeoutTimestamp: packetTimeout,
				Signer:           ts.cosmosDeployer.FormattedAddress(),
				Payloads: []channeltypesv2.Payload{
					{
						SourcePort:      transfertypes.PortID,
						DestinationPort: transfertypes.PortID,
						Version:         transfertypes.V1,
						Encoding:        transfertypes.EncodingABI,
						Value:           must(transfertypes.EncodeABIFungibleTokenPacketData(&transferPayload)),
					},
				},
			}

			// broadcast and retrieve tx hash so further relaying
			resp, err := ts.base.BroadcastMessages(ctx, cosmosChain, ts.cosmosDeployer, 200_000, &msgSendPacket)
			require.NoError(t, err, "unable to broadcast messages")
			require.NotEmpty(t, resp.TxHash, "tx hash is empty")

			cosmosSendTxHash, err = hex.DecodeString(resp.TxHash)
			require.NoError(t, err, "unable to decode tx hash")
		})

		ts.do("2: Verify balances on Cosmos", func() {
			req := &banktypes.QueryBalanceRequest{
				Address: ts.cosmosDeployer.FormattedAddress(),
				Denom:   transferCoin.Denom,
			}

			resp, err := e2esuite.GRPCQuery[banktypes.QueryBalanceResponse](ctx, cosmosChain, req)

			require.NoError(t, err, "unable to query balance")
			require.NotNil(t, resp.Balance, "balance is nil")
			require.Equal(t, testvalues.InitialBalance-testvalues.TransferAmount, resp.Balance.Amount.Int64())
		})

		ts.do("3: Verify commitment exists on Cosmos", func() {
			req := &channeltypesv2.QueryPacketCommitmentRequest{
				ClientId: testvalues.FirstWasmClientID,
				Sequence: 1,
			}

			resp, err := e2esuite.GRPCQuery[channeltypesv2.QueryPacketCommitmentResponse](ctx, cosmosChain, req)

			require.NoError(t, err)
			require.NotEmpty(t, resp.Commitment)

			t.Logf("Cosmos transfer packet commitment: 0x%x", resp.Commitment)
		})

		var cosmosToEVMTxBody []byte

		ts.do("4: Prepare relay tx from Cosmos to EVM", func() {
			req := &relayertypes.RelayByTxRequest{
				SrcChain:    cosmosChain.Config().ChainID,
				DstChain:    evmChain.ChainID.String(),
				SourceTxIds: [][]byte{cosmosSendTxHash},
				SrcClientId: testvalues.FirstWasmClientID,
				DstClientId: testvalues.CustomClientID,
			}
			resp, err := ts.relayerClient.RelayByTx(ts.ctx, req)

			require.NoError(t, err, "unable to retrieve relay tx")
			require.NotEmpty(t, resp.Tx, "relay tx is empty")

			cosmosToEVMTxBody = resp.Tx
		})

		var packet ics26router.IICS26RouterMsgsPacket

		ts.do("5: Broadcast relay tx from Cosmos to EVM", func() {
			receipt, err := evmChain.BroadcastTx(
				ctx,
				ts.evmDeployer,
				5_000_000,
				&ts.evmBindings.ICS26RouterAddress,
				cosmosToEVMTxBody,
			)
			require.NoError(t, err)
			require.Equal(t, ethtypes.ReceiptStatusSuccessful, receipt.Status, "relay tx failed: %+v", receipt)

			ethReceiveAckEvent, err := e2esuite.GetEvmEvent(
				receipt,
				ts.evmBindings.ICS26Router.ParseWriteAcknowledgement,
			)
			require.NoError(t, err, "unable to get write acknowledgement event")

			packet = ethReceiveAckEvent.Packet
		})

		ts.do("6: Verify balances on EVM", func() {
			// Recreate the full denom path
			denomOnEVM := transfertypes.NewDenom(
				transferCoin.Denom,
				transfertypes.NewHop(packet.Payloads[0].DestPort, packet.DestClient),
			)

			// create ibcERC20 contract
			ibcERC20Address, err := ts.evmBindings.ICS20Transfer.IbcERC20Contract(nil, denomOnEVM.Path())
			require.NoError(t, err, "unable to get ibcERC20 contract address")

			ibcERC20, err := ibcerc20.NewContract(ibcERC20Address, evmChain.RPCClient)
			require.NoError(t, err)

			// sanity checks
			actualDenom, err := ibcERC20.Name(nil)
			require.NoError(t, err)
			require.Equal(t, denomOnEVM.Path(), actualDenom)

			actualSymbol, err := ibcERC20.Symbol(nil)
			require.NoError(t, err)
			require.Equal(t, denomOnEVM.Path(), actualSymbol)

			actualFullDenom, err := ibcERC20.FullDenomPath(nil)
			require.NoError(t, err)
			require.Equal(t, denomOnEVM.Path(), actualFullDenom)

			// User balance on Ethereum
			userBalance, err := ibcERC20.BalanceOf(nil, evmDeployerAddr)
			require.NoError(t, err)
			require.Equal(t, transferAmount, userBalance)

			// ICS20 contract balance on Ethereum
			ics20TransferBalance, err := ibcERC20.BalanceOf(nil, ibcERC20Address)
			require.NoError(t, err)
			require.Zero(t, ics20TransferBalance.Int64())
		})
	})
}

type cosmosToEVMAttestorTestSuite struct {
	*testing.T

	ctx context.Context

	base *e2esuite.TestSuite

	// users
	evmDeployer    *ecdsa.PrivateKey
	cosmosDeployer ibc.Wallet

	// clients
	attestorClient attestortypes.AttestationServiceClient
	relayerClient  relayertypes.RelayerServiceClient

	// evmBindings
	evmBindings evmBindings
}

func newCosmosToEVMAttestorTestSuite(t *testing.T) *cosmosToEVMAttestorTestSuite {
	t.Helper()

	// note: this is really bad, but other tests internals expect chdir
	require.NoError(t, os.Chdir("../.."))

	ctx := context.Background()

	// 1. Ensure some ENV
	var (
		envEthWasmType    = testvalues.EnvEnsure(testvalues.EnvKeyEthLcOnCosmos, testvalues.EthWasmTypeAttestorWasm)
		envEthTestnetType = testvalues.EnvEnsure(testvalues.EnvKeyEthTestnetType, testvalues.EthTestnetTypeOptimism)
		_                 = testvalues.EnvEnsure(testvalues.EnvKeyRustLog, testvalues.EnvValueRustLog_Info)
	)

	// Skip test if not relevant
	if envEthWasmType != testvalues.EthWasmTypeAttestorWasm && envEthWasmType != testvalues.EthWasmTypeAttestorNative {
		t.Skipf(
			"Skipping: expecting %s to be %q or %q, got %q",
			testvalues.EnvKeyEthLcOnCosmos,
			testvalues.EthWasmTypeAttestorWasm,
			testvalues.EthWasmTypeAttestorNative,
			envEthWasmType,
		)
	}

	if envEthTestnetType != testvalues.EthTestnetTypeOptimism {
		t.Skipf(
			"Skipping: expecting %s to be %q, got %q",
			testvalues.EnvKeyEthTestnetType,
			testvalues.EthTestnetTypeOptimism,
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
		evmChain    = base.EthChains[0]
		cosmosChain = base.CosmosChains[0]
	)

	// Set some ENV related to RPC
	os.Setenv(testvalues.EnvKeyEthRPC, evmChain.RPC)
	os.Setenv(testvalues.EnvKeyTendermintRPC, cosmosChain.GetHostRPCAddress())

	// 3. Provision users
	evmDeployer, err := evmChain.CreateAndFundUser()
	require.NoError(t, err, "unable to provision EVM deployer")

	cosmosDeployer := base.CreateAndFundCosmosUser(ctx, cosmosChain)

	// 4. Setup ONE cosmos attestor in Docker
	// TODO: support for arbitrary number of attestors in the future with
	// TODO: private keys provisioning on the fly
	attestorClient, attestorAddressStr, attestorEndpoint := setupCosmosAttestor(ctx, t, base, cosmosChain)

	attestorAddress := ethcommon.HexToAddress(attestorAddressStr)
	require.NotEqual(t, ethcommon.Address{}, attestorAddress, "attestor address should not be empty")

	t.Logf("Attestor address: %s", attestorAddress.Hex())

	// 4. Deploy IBC contracts
	out, err := base.EthChains[0].ForgeScript(evmDeployer, testvalues.E2EDeployScriptPath)
	require.NoError(t, err, "unable to deploy ibc contracts")

	evmContracts := extractEVMBindings(t, out, base.EthChains[0].RPCClient)

	// 5. Start the relayer with the actual attestor endpoint
	aggConfig := relayer.DefaultAggregatorConfig()
	aggConfig.Attestor.AttestorEndpoints = []string{attestorEndpoint}
	relayerClient := runRelayer(t, relayer.NewConfig([]relayer.ModuleConfig{
		{
			Name:     relayer.ModuleCosmosToEth,
			SrcChain: cosmosChain.Config().ChainID,
			DstChain: evmChain.ChainID.String(),
			Config: relayer.CosmosToEthModuleConfig{
				TmRpcUrl:     cosmosChain.GetHostRPCAddress(),
				Ics26Address: evmContracts.ICS26RouterAddress.Hex(),
				EthRpcUrl:    evmChain.RPC,
				Mode: relayer.CosmosToEthTxBuilderMode{
					Type:             "attested",
					AggregatorConfig: &aggConfig,
				},
			},
		},
		{
			Name:     relayer.ModuleEthToCosmos,
			SrcChain: evmChain.ChainID.String(),
			DstChain: cosmosChain.Config().ChainID,
			Config: relayer.EthToCosmosModuleConfig{
				Ics26Address:  evmContracts.ICS26RouterAddress.String(),
				TmRpcUrl:      cosmosChain.GetHostRPCAddress(),
				EthRpcUrl:     evmChain.RPC,
				SignerAddress: cosmosDeployer.FormattedAddress(),
				Mode: relayer.EthToCosmosTxBuilderMode{
					Type:             "attested",
					AggregatorConfig: &aggConfig,
				},
			},
		},
	}))

	// 6. Deploy Cosmos LC on EVM (relayer creates the tx, evmDeployer broadcasts it)
	latestCosmosHeader, err := cosmosChain.GetFullNode().Client.Header(ctx, nil)
	require.NoError(t, err, "unable to get latest cosmos header")

	cosmosLCParams := map[string]string{
		// see contracts/light-clients/AttestorLightClient.sol constructor(...)
		testvalues.ParameterKey_AttestorAddresses: attestorAddress.Hex(),
		testvalues.ParameterKey_MinRequiredSigs:   strconv.Itoa(testvalues.DefaultMinRequiredSigs),
		testvalues.ParameterKey_height:            strconv.FormatInt(latestCosmosHeader.Header.Height, 10),
		testvalues.ParameterKey_timestamp:         strconv.FormatInt(latestCosmosHeader.Header.Time.Unix(), 10),
		// Light client proof submission is executed by ICS26Router; grant role to router
		testvalues.ParameterKey_RoleManager: evmContracts.ICS26RouterAddress.Hex(),
	}

	t.Logf("Cosmos LC params: %+v", cosmosLCParams)

	resp, err := relayerClient.CreateClient(ctx, &relayertypes.CreateClientRequest{
		SrcChain:   cosmosChain.Config().ChainID,
		DstChain:   evmChain.ChainID.String(),
		Parameters: cosmosLCParams,
	})

	require.NoError(t, err, "unable to create cosmos light-client tx")
	require.NotEmpty(t, resp.Tx, "tx is empty")

	txReceipt, err := evmChain.BroadcastTx(ctx, evmDeployer, 15_000_000, nil, resp.Tx)
	require.NoError(t, err, "unable to broadcast cosmos light-client tx on evm")
	require.Equal(t, ethtypes.ReceiptStatusSuccessful, txReceipt.Status, "tx failed: %+v", txReceipt)

	evmContracts.LightClientAddress = txReceipt.ContractAddress
	evmContracts.LightClient, err = attestation.NewContract(txReceipt.ContractAddress, evmChain.RPCClient)
	require.NoError(t, err, "unable to create cosmos light-client wrapper")

	// 7. Deploy EVM LC on Cosmos (relayer creates the tx, cosmosSender broadcasts it)
	checksumHex := base.StoreLightClient(ctx, cosmosChain, cosmosDeployer)
	require.NotEmpty(t, checksumHex, "checksumHex is empty")

	evmBlockHeader, err := evmChain.RPCClient.HeaderByNumber(ctx, nil)
	require.NoError(t, err, "unable to get evm block header")

	evmLCParams := map[string]string{
		testvalues.ParameterKey_ChecksumHex:       checksumHex,
		testvalues.ParameterKey_AttestorAddresses: attestorAddress.Hex(),
		testvalues.ParameterKey_MinRequiredSigs:   strconv.Itoa(testvalues.DefaultMinRequiredSigs),
		testvalues.ParameterKey_height:            strconv.FormatInt(evmBlockHeader.Number.Int64(), 10),
		testvalues.ParameterKey_timestamp:         fmt.Sprintf("%d", evmBlockHeader.Time),
	}

	t.Logf("EVM LC params: %+v", evmLCParams)

	resp, err = relayerClient.CreateClient(ctx, &relayertypes.CreateClientRequest{
		SrcChain:   evmChain.ChainID.String(),
		DstChain:   cosmosChain.Config().ChainID,
		Parameters: evmLCParams,
	})

	require.NoError(t, err, "unable to create evm light-client tx")
	require.NotEmpty(t, resp.Tx, "tx is empty")

	cosmosResp := base.MustBroadcastSdkTxBody(ctx, cosmosChain, cosmosDeployer, 20_000_000, resp.Tx)
	wasmClientID, err := cosmosutils.GetEventValue(
		cosmosResp.Events,
		clienttypes.EventTypeCreateClient,
		clienttypes.AttributeKeyClientID,
	)

	require.NoError(t, err, "unable to get event value from create client tx")
	require.Equal(t, testvalues.FirstWasmClientID, wasmClientID)

	// 8. Register counter parties
	// EVM
	evmRegistrationTx, err := evmContracts.ICS26Router.AddClient(
		must(evmChain.GetTransactOpts(evmDeployer)),
		testvalues.CustomClientID,
		ics26router.IICS02ClientMsgsCounterpartyInfo{
			ClientId:     wasmClientID,
			MerklePrefix: [][]byte{[]byte("")},
		},
		evmContracts.LightClientAddress,
	)
	require.NoError(t, err, "unable to add registration counterparty on EVM")

	evmRegistrationReceipt, err := evmChain.GetTxReciept(ctx, evmRegistrationTx.Hash())
	require.NoError(t, err, "unable to get registration client receipt on EVM")

	event, err := e2esuite.GetEvmEvent(evmRegistrationReceipt, evmContracts.ICS26Router.ParseICS02ClientAdded)
	require.NoError(t, err, "unable to get registration client event on EVM")
	require.Equal(t, testvalues.CustomClientID, event.ClientId)
	require.Equal(t, wasmClientID, event.CounterpartyInfo.ClientId)

	// Cosmos
	_, err = base.BroadcastMessages(ctx, cosmosChain, cosmosDeployer, 200_000, &clienttypesv2.MsgRegisterCounterparty{
		ClientId:                 wasmClientID,
		CounterpartyMerklePrefix: [][]byte{[]byte("")},
		CounterpartyClientId:     testvalues.CustomClientID,
		Signer:                   cosmosDeployer.FormattedAddress(),
	})
	require.NoError(t, err, "unable to register counterparty on Cosmos")

	return &cosmosToEVMAttestorTestSuite{
		T: t,

		ctx:  ctx,
		base: base,

		evmDeployer:    evmDeployer,
		cosmosDeployer: cosmosDeployer,

		attestorClient: attestorClient,
		relayerClient:  relayerClient,

		evmBindings: evmContracts,
	}
}

func (ts *cosmosToEVMAttestorTestSuite) do(name string, fn func()) {
	ts.Logf("Running step %q", name)
	start := time.Now()

	fn()

	ts.Logf("Step %q completed in %s", name, time.Since(start))
}

// setupCosmosAttestor starts a single Cosmos attestor in Docker.
// Returns the attestor client, its Ethereum address, and its endpoint.
func setupCosmosAttestor(
	ctx context.Context,
	t *testing.T,
	base *e2esuite.TestSuite,
	cosmosChain *cosmos.CosmosChain,
) (attestortypes.AttestationServiceClient, string, string) {
	t.Helper()

	// Start single Cosmos attestor using Docker
	result := attestor.SetupCosmosAttestors(
		ctx,
		t,
		base.GetDockerClient(),
		base.GetNetworkID(),
		cosmosChain.GetRPCAddress(),
	)

	// Verify we have exactly one attestor
	require.Equal(t, 1, len(result.Addresses), "expected exactly 1 attestor")
	require.Equal(t, 1, len(result.Endpoints), "expected exactly 1 endpoint")

	// Get the attestor client
	attestorClient, err := attestor.GetAttestationServiceClient(result.Endpoints[0])
	require.NoError(t, err, "unable to get attestation service client")

	t.Logf("Cosmos attestor started: address=%s, endpoint=%s", result.Addresses[0], result.Endpoints[0])

	return attestorClient, result.Addresses[0], result.Endpoints[0]
}

func runRelayer(t *testing.T, config relayer.Config) relayertypes.RelayerServiceClient {
	t.Helper()

	err := config.GenerateConfigFile(testvalues.RelayerConfigFilePath)
	require.NoError(t, err, "unable to generate relayer config file")

	proc, err := relayer.StartRelayer(testvalues.RelayerConfigFilePath)
	require.NoError(t, err, "unable to start relayer")

	t.Cleanup(func() {
		os.Remove(testvalues.RelayerConfigFilePath)
		if err := proc.Kill(); err != nil {
			t.Logf("unable to kill relayer process: %v", err)
		}
	})

	client, err := relayer.GetGRPCClient(relayer.DefaultRelayerGRPCAddress())
	require.NoError(t, err, "unable to get relayer client")

	return client
}

type evmBindings struct {
	ICS26RouterAddress ethcommon.Address
	ICS26Router        *ics26router.Contract

	ICS20TransferAddress ethcommon.Address
	ICS20Transfer        *ics20transfer.Contract

	ERC20Address ethcommon.Address
	ERC20        *erc20.Contract

	LightClientAddress ethcommon.Address
	LightClient        *attestation.Contract
}

// Parses stdout of `scripts/E2ETestDeploy.s.sol` into a set of contract wrappers
func extractEVMBindings(t *testing.T, raw []byte, evmClient *ethclient.Client) evmBindings {
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

	return evmBindings{
		ICS26RouterAddress: ics26RouterAddress,
		ICS26Router:        ics26Contract,

		ICS20TransferAddress: ics20TransferAddress,
		ICS20Transfer:        ics20Contract,

		ERC20Address: erc20Address,
		ERC20:        erc20Contract,

		// will be set later
		LightClientAddress: ethcommon.Address{},
		LightClient:        nil,
	}
}

func must[T any](value T, err error) T {
	if err != nil {
		panic(err)
	}

	return value
}
