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

	"github.com/cosmos/interchaintest/v10/ibc"
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
	ibcexported "github.com/cosmos/ibc-go/v10/modules/core/exported"

	"github.com/cosmos/solidity-ibc-eureka/packages/go-abigen/attestorlightclient"
	"github.com/cosmos/solidity-ibc-eureka/packages/go-abigen/ibcerc20"
	"github.com/cosmos/solidity-ibc-eureka/packages/go-abigen/ics20transfer"
	"github.com/cosmos/solidity-ibc-eureka/packages/go-abigen/ics26router"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/attestor"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/cosmos"
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

			evmChain    = ts.base.EthChain
			cosmosChain = ts.base.CosmosChains[0]

			evmDeployerAddr = crypto.PubkeyToAddress(ts.evmDeployer.PublicKey)

			transferAmount = big.NewInt(tv.TransferAmount)
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
				SourceClient:     tv.FirstWasmClientID,
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
			require.Equal(t, tv.InitialBalance-tv.TransferAmount, resp.Balance.Amount.Int64())
		})

		ts.do("3: Verify commitment exists on Cosmos", func() {
			req := &channeltypesv2.QueryPacketCommitmentRequest{
				ClientId: tv.FirstWasmClientID,
				Sequence: 1,
			}

			resp, err := e2esuite.GRPCQuery[channeltypesv2.QueryPacketCommitmentResponse](ctx, cosmosChain, req)

			require.NoError(t, err)
			require.NotEmpty(t, resp.Commitment)
		})

		var cosmosToEVMTxBody []byte

		ts.do("4: Prepare relay tx from Cosmos to EVM", func() {
			req := &relayertypes.RelayByTxRequest{
				SrcChain:    cosmosChain.Config().ChainID,
				DstChain:    evmChain.ChainID.String(),
				SourceTxIds: [][]byte{cosmosSendTxHash},
				SrcClientId: tv.FirstWasmClientID,
				DstClientId: tv.CustomClientID,
			}
			resp, err := ts.relayerClient.RelayByTx(ts.ctx, req)

			require.NoError(t, err, "unable to retrieve relay tx")
			require.NotEmpty(t, resp.Tx, "relay tx is empty")

			cosmosToEVMTxBody = resp.Tx
		})

		var (
			packet    ics26router.IICS26RouterMsgsPacket
			ackTxHash []byte
		)

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
			ackTxHash = receipt.TxHash.Bytes()
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

		// Now we want to acknowledge the packet on Cosmos
		var evmToCosmosTxBody []byte

		ts.do("7: Prepare ACK relay tx from EVM to Cosmos", func() {
			req := &relayertypes.RelayByTxRequest{
				SrcChain:    evmChain.ChainID.String(),
				DstChain:    cosmosChain.Config().ChainID,
				SourceTxIds: [][]byte{ackTxHash},
				SrcClientId: tv.CustomClientID,
				DstClientId: tv.FirstWasmClientID,
			}

			resp, err := ts.relayerClient.RelayByTx(ctx, req)

			require.NoError(t, err)
			require.NotEmpty(t, resp.Tx)
			require.Empty(t, resp.Address)

			evmToCosmosTxBody = resp.Tx
		})

		ts.do("8: Broadcast ACK relay tx from EVM to Cosmos", func() {
			resp := ts.base.MustBroadcastSdkTxBody(ctx, cosmosChain, ts.cosmosDeployer, 2_000_000, evmToCosmosTxBody)

			txHash, err := hex.DecodeString(resp.TxHash)
			require.NoError(t, err)
			require.NotEmpty(t, txHash)
		})

		ts.do("9: Verify commitment removed from Cosmos", func() {
			req := &channeltypesv2.QueryPacketCommitmentRequest{
				ClientId: tv.FirstWasmClientID,
				Sequence: 1,
			}

			_, err := e2esuite.GRPCQuery[channeltypesv2.QueryPacketCommitmentResponse](ctx, cosmosChain, req)

			require.ErrorContains(t, err, "packet commitment hash not found")
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

	cosmosDeployer := base.CreateAndFundCosmosUser(ctx, cosmosChain)

	// 4. Setup ONE cosmos attestor (for the sake of the current test)
	// TODO: support for arbitrary number of attestors in the future with
	// TODO: private keys provisioning on the fly
	const attestorPort = 9000

	attestorServerAddr := runAttestor(t, attestor.CosmosBinary, func(c *attestor.AttestorConfig) string {
		c.Server.Port = attestorPort
		c.Cosmos.URL = cosmosChain.GetHostRPCAddress()

		return "/tmp/attestor_0.toml"
	})

	attestorClient, err := attestor.GetAttestationServiceClient(attestorServerAddr)
	require.NoError(t, err, "unable to get attestation service client")

	// evm address for Cosmos attestor
	attestorAddress, err := attestor.ReadAttestorAddress(attestor.CosmosBinary)
	require.NoError(t, err, "unable to read attestor address")

	// 4. Deploy IBC contracts
	out, err := base.EthChain.ForgeScript(evmDeployer, tv.E2EDeployScriptPath)
	require.NoError(t, err, "unable to deploy ibc contracts")

	evmContracts := extractEVMBindings(t, out, base.EthChain.RPCClient)

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
				Ics26Address:     evmContracts.ICS26RouterAddress.Hex(),
				EthRpcUrl:        evmChain.RPC,
			},
		},
		{
			Name:     relayer.ModuleEthToCosmosAttested,
			SrcChain: evmChain.ChainID.String(),
			DstChain: cosmosChain.Config().ChainID,
			Config: relayer.EthToCosmosAttestedModuleConfig{
				AttestedChainId:  evmChain.ChainID.String(),
				AggregatorConfig: relayer.DefaultAggregatorConfig(),
				AttestedRpcUrl:   evmChain.RPC,
				Ics26Address:     evmContracts.ICS26RouterAddress.String(),
				TmRpcUrl:         cosmosChain.GetHostRPCAddress(),
				SignerAddress:    cosmosDeployer.FormattedAddress(),
			},
		},
	}))

	// 6. Deploy Cosmos LC on EVM (relayer creates the tx, evmDeployer broadcasts it)
	latestCosmosHeader, err := cosmosChain.GetFullNode().Client.Header(ctx, nil)
	require.NoError(t, err, "unable to get latest cosmos header")

	var (
		cosmosBlockHeight    = latestCosmosHeader.Header.Height
		cosmosBlockTimestamp = latestCosmosHeader.Header.Time.Unix()
	)

	resp, err := relayerClient.CreateClient(ctx, &relayertypes.CreateClientRequest{
		SrcChain: cosmosChain.Config().ChainID,
		DstChain: evmChain.ChainID.String(),
		Parameters: map[string]string{
			// see contracts/light-clients/AttestorLightClient.sol constructor(...)
			tv.ParameterKey_AttestorAddresses: ethcommon.HexToAddress(attestorAddress).Hex(),
			tv.ParameterKey_MinRequiredSigs:   "1",
			tv.ParameterKey_height:            strconv.FormatInt(cosmosBlockHeight, 10),
			tv.ParameterKey_timestamp:         strconv.FormatInt(cosmosBlockTimestamp, 10),
			// Light client proof submission is executed by ICS26Router; grant role to router
			tv.ParameterKey_RoleManager: evmContracts.ICS26RouterAddress.Hex(),
		},
	})

	require.NoError(t, err, "unable to create cosmos light-client tx")
	require.NotEmpty(t, resp.Tx, "tx is empty")

	txReceipt, err := evmChain.BroadcastTx(ctx, evmDeployer, 15_000_000, nil, resp.Tx)
	require.NoError(t, err, "unable to broadcast cosmos light-client tx on evm")
	require.Equal(t, ethtypes.ReceiptStatusSuccessful, txReceipt.Status, "tx failed: %+v", txReceipt)

	evmContracts.LightClientAddress = txReceipt.ContractAddress
	evmContracts.LightClient, err = attestorlightclient.NewContract(txReceipt.ContractAddress, evmChain.RPCClient)
	require.NoError(t, err, "unable to create cosmos light-client wrapper")

	// 7. Deploy EVM LC on Cosmos (relayer creates the tx, cosmosSender broadcasts it)
	checksumHex := base.StoreLightClient(ctx, cosmosChain, cosmosDeployer)
	require.NotEmpty(t, checksumHex, "checksumHex is empty")

	evmBlockHeader, err := evmChain.RPCClient.HeaderByNumber(ctx, nil)
	require.NoError(t, err, "unable to get evm block header")

	var (
		evmBlockHeight    = evmBlockHeader.Number.Int64()
		evmBlockTimestamp = evmBlockHeader.Time
	)

	resp, err = relayerClient.CreateClient(ctx, &relayertypes.CreateClientRequest{
		SrcChain: evmChain.ChainID.String(),
		DstChain: cosmosChain.Config().ChainID,
		Parameters: map[string]string{
			tv.ParameterKey_ChecksumHex:       checksumHex,
			tv.ParameterKey_AttestorAddresses: ethcommon.HexToAddress(attestorAddress).Hex(),
			tv.ParameterKey_MinRequiredSigs:   "1",
			tv.ParameterKey_height:            strconv.FormatInt(evmBlockHeight, 10),
			tv.ParameterKey_timestamp:         fmt.Sprintf("%d", evmBlockTimestamp),
		},
	})
	require.NoError(t, err, "unable to create evm light-client tx")
	require.NotEmpty(t, resp.Tx, "tx is empty")

	cosmosResp := base.MustBroadcastSdkTxBody(ctx, cosmosChain, cosmosDeployer, 20_000_000, resp.Tx)
	wasmClientID, err := cosmos.GetEventValue(
		cosmosResp.Events,
		clienttypes.EventTypeCreateClient,
		clienttypes.AttributeKeyClientID,
	)

	require.NoError(t, err, "unable to get event value from create client tx")
	require.Equal(t, tv.FirstWasmClientID, wasmClientID)

	// 8. Register counter parties
	// EVM
	evmRegistrationTx, err := evmContracts.ICS26Router.AddClient(
		must(evmChain.GetTransactOpts(evmDeployer)),
		tv.CustomClientID,
		ics26router.IICS02ClientMsgsCounterpartyInfo{
			ClientId:     wasmClientID,
			MerklePrefix: [][]byte{[]byte(ibcexported.StoreKey), []byte("")},
		},
		evmContracts.LightClientAddress,
	)
	require.NoError(t, err, "unable to add registration counterparty on EVM")

	evmRegistrationReceipt, err := evmChain.GetTxReciept(ctx, evmRegistrationTx.Hash())
	require.NoError(t, err, "unable to get registration client receipt on EVM")

	event, err := e2esuite.GetEvmEvent(evmRegistrationReceipt, evmContracts.ICS26Router.ParseICS02ClientAdded)
	require.NoError(t, err, "unable to get registration client event on EVM")
	require.Equal(t, tv.CustomClientID, event.ClientId)
	require.Equal(t, wasmClientID, event.CounterpartyInfo.ClientId)

	// Cosmos
	_, err = base.BroadcastMessages(ctx, cosmosChain, cosmosDeployer, 200_000, &clienttypesv2.MsgRegisterCounterparty{
		ClientId:                 wasmClientID,
		CounterpartyMerklePrefix: [][]byte{[]byte("")},
		CounterpartyClientId:     tv.CustomClientID,
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

type evmBindings struct {
	ICS26RouterAddress ethcommon.Address
	ICS26Router        *ics26router.Contract

	ICS20TransferAddress ethcommon.Address
	ICS20Transfer        *ics20transfer.Contract

	ERC20Address ethcommon.Address
	ERC20        *erc20.Contract

	LightClientAddress ethcommon.Address
	LightClient        *attestorlightclient.Contract
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
