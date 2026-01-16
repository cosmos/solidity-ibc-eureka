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

	"github.com/stretchr/testify/suite"

	"github.com/ethereum/go-ethereum/accounts/abi"
	ethcommon "github.com/ethereum/go-ethereum/common"
	ethtypes "github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/crypto"

	banktypes "github.com/cosmos/cosmos-sdk/x/bank/types"

	transfertypes "github.com/cosmos/ibc-go/v10/modules/apps/transfer/types"
	clienttypes "github.com/cosmos/ibc-go/v10/modules/core/02-client/types"
	clienttypesv2 "github.com/cosmos/ibc-go/v10/modules/core/02-client/v2/types"
	ibcexported "github.com/cosmos/ibc-go/v10/modules/core/exported"

	"github.com/cosmos/interchaintest/v10/ibc"

	"github.com/cosmos/solidity-ibc-eureka/packages/go-abigen/ics20transfer"
	"github.com/cosmos/solidity-ibc-eureka/packages/go-abigen/ics26router"
	"github.com/cosmos/solidity-ibc-eureka/packages/go-abigen/sp1ics07tendermint"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/attestor"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/cosmos"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/e2esuite"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/ethereum"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/relayer"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types"
	aggregatortypes "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/aggregator"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/erc20"
	relayertypes "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/relayer"
)

// IbcAttestorTestSuite is a suite of tests that wraps TestSuite
// and can provide additional functionality
type IbcAttestorTestSuite struct {
	e2esuite.TestSuite

	// Whether to generate fixtures for tests or not
	solidityFixtureGenerator *types.SolidityFixtureGenerator
	wasmFixtureGenerator     *types.WasmFixtureGenerator

	// The private key of a test account
	key *ecdsa.PrivateKey
	// The private key of the faucet account of interchaintest
	deployer *ecdsa.PrivateKey

	contractAddresses ethereum.DeployedContracts
	sp1Ics07Address   ethcommon.Address

	sp1Ics07Contract *sp1ics07tendermint.Contract
	ics26Contract    *ics26router.Contract
	ics20Contract    *ics20transfer.Contract
	erc20Contract    *erc20.Contract

	RelayerClient relayertypes.RelayerServiceClient

	SimdRelayerSubmitter ibc.Wallet
	EthRelayerSubmitter  *ecdsa.PrivateKey

	AggregatorClient aggregatortypes.AggregatorServiceClient
}

// TestWithIbcAttestorTestSuite is the boilerplate code that allows the test suite to be run
func TestWithIbcAttestorTestSuite(t *testing.T) {
	suite.Run(t, new(IbcAttestorTestSuite))
}

// SetupSuite calls the underlying IbcAttestorTestSuite's SetupSuite method
// and deploys the IbcEureka contract
func (s *IbcAttestorTestSuite) SetupSuite(ctx context.Context, proofType types.SupportedProofType, chainType attestor.ChainType) {
	if s.EthWasmType == "" {
		s.EthWasmType = os.Getenv(testvalues.EnvKeyEthLcOnCosmos)
		if s.EthWasmType != testvalues.EthWasmTypeAttestorWasm && s.EthWasmType != testvalues.EthWasmTypeAttestorNative {
			s.T().Fatalf("attestor tests must use attestor type (attestor-wasm or attestor-native), found %s", s.EthWasmType)
		}
		s.T().Logf("eth lc on cosmos type: %s", s.EthWasmType)
	}

	s.TestSuite.SetupSuite(ctx)

	if os.Getenv(testvalues.EnvKeyRustLog) == "" {
		os.Setenv(testvalues.EnvKeyRustLog, testvalues.EnvValueRustLog_Info)
	}

	eth, simd := s.EthChains[0], s.CosmosChains[0]

	s.T().Logf("Setting up the test suite with proof type: %s", proofType.String())

	var prover string
	s.Require().True(s.Run("Set up environment", func() {
		err := os.Chdir("../..")
		s.Require().NoError(err)

		s.key, err = eth.CreateAndFundUser()
		s.Require().NoError(err)

		s.EthRelayerSubmitter, err = eth.CreateAndFundUser()
		s.Require().NoError(err)

		operatorKey, err := eth.CreateAndFundUser()
		s.Require().NoError(err)

		s.deployer, err = eth.CreateAndFundUser()
		s.Require().NoError(err)

		s.SimdRelayerSubmitter = s.CreateAndFundCosmosUser(ctx, simd)

		prover = os.Getenv(testvalues.EnvKeySp1Prover)
		switch prover {
		case "", testvalues.EnvValueSp1Prover_Mock:
			s.T().Logf("Using mock prover")
			prover = testvalues.EnvValueSp1Prover_Mock
			os.Setenv(testvalues.EnvKeySp1Prover, testvalues.EnvValueSp1Prover_Mock)
			os.Setenv(testvalues.EnvKeyVerifier, testvalues.EnvValueVerifier_Mock)

			s.Require().Empty(
				os.Getenv(testvalues.EnvKeyGenerateSolidityFixtures),
				"Fixtures are not supported for mock prover",
			)
		case testvalues.EnvValueSp1Prover_Network:
			s.Require().Empty(
				os.Getenv(testvalues.EnvKeyVerifier),
				fmt.Sprintf("%s should not be set when using the network prover in e2e tests.", testvalues.EnvKeyVerifier),
			)
			// make sure that the NETWORK_PRIVATE_KEY is set.
			s.Require().NotEmpty(os.Getenv(testvalues.EnvKeyNetworkPrivateKey))
		default:
			s.Require().Fail("invalid prover type: %s", prover)
		}

		os.Setenv(testvalues.EnvKeyEthRPC, eth.RPC)
		os.Setenv(testvalues.EnvKeyTendermintRPC, simd.GetHostRPCAddress())
		os.Setenv(testvalues.EnvKeySp1Prover, prover)
		os.Setenv(testvalues.EnvKeyOperatorPrivateKey, hex.EncodeToString(crypto.FromECDSA(operatorKey)))
	}))

	// Needs to be added here so the cleanup is called after the test suite is done
	s.wasmFixtureGenerator = types.NewWasmFixtureGenerator(&s.Suite)
	s.solidityFixtureGenerator = types.NewSolidityFixtureGenerator()

	s.Require().True(s.Run("Deploy IBC contracts", func() {
		stdout, err := eth.ForgeScript(s.deployer, testvalues.E2EDeployScriptPath)
		s.Require().NoError(err)

		s.contractAddresses, err = ethereum.GetEthContractsFromDeployOutput(string(stdout))
		s.Require().NoError(err)
		s.ics26Contract, err = ics26router.NewContract(ethcommon.HexToAddress(s.contractAddresses.Ics26Router), eth.RPCClient)
		s.Require().NoError(err)
		s.ics20Contract, err = ics20transfer.NewContract(ethcommon.HexToAddress(s.contractAddresses.Ics20Transfer), eth.RPCClient)
		s.Require().NoError(err)
		s.erc20Contract, err = erc20.NewContract(ethcommon.HexToAddress(s.contractAddresses.Erc20), eth.RPCClient)
		s.Require().NoError(err)
	}))

	var attestorResult attestor.SetupResult
	s.Require().True(s.Run("Setup Ethereum attestors", func() {
		attestorResult = attestor.SetupEthAttestors(ctx, s.T(), eth.RPC, s.contractAddresses.Ics26Router, chainType)
	}))

	s.T().Cleanup(func() {
		attestor.CleanupProcesses(s.T(), attestorResult.Processes)
	})

	var relayerProcess *os.Process
	s.Require().True(s.Run("Start Relayer", func() {
		// Eth→Cosmos uses attestor, Cosmos→Eth uses SP1
		config := relayer.NewConfigBuilder().
			EthToCosmosAttested(relayer.EthToCosmosAttestedParams{
				EthChainID:    eth.ChainID.String(),
				CosmosChainID: simd.Config().ChainID,
				TmRPC:         simd.GetHostRPCAddress(),
				ICS26Address:  s.contractAddresses.Ics26Router,
				EthRPC:        eth.RPC,
				SignerAddress: s.SimdRelayerSubmitter.FormattedAddress(),
			}).
			CosmosToEthSP1(relayer.CosmosToEthSP1Params{
				CosmosChainID: simd.Config().ChainID,
				EthChainID:    eth.ChainID.String(),
				TmRPC:         simd.GetHostRPCAddress(),
				ICS26Address:  s.contractAddresses.Ics26Router,
				EthRPC:        eth.RPC,
				Prover: relayer.SP1ProverConfig{
					Type:           prover,
					PrivateCluster: os.Getenv(testvalues.EnvKeyNetworkPrivateCluster) == testvalues.EnvValueSp1Prover_PrivateCluster,
				},
			}).
			Build()

		s.T().Logf("relayer config %v", config)

		err := config.GenerateConfigFile(testvalues.RelayerConfigFilePath)
		s.Require().NoError(err)

		relayerProcess, err = relayer.StartRelayer(testvalues.RelayerConfigFilePath)
		s.Require().NoError(err)

		s.T().Cleanup(func() {
			os.Remove(testvalues.RelayerConfigFilePath)
		})
	}))

	s.T().Cleanup(func() {
		if relayerProcess != nil {
			err := relayerProcess.Kill()
			if err != nil {
				s.T().Logf("Failed to kill the relayer process: %v", err)
			}
		}
	})

	s.Require().True(s.Run("Create Relayer Client", func() {
		var err error
		s.RelayerClient, err = relayer.GetGRPCClient(relayer.DefaultRelayerGRPCAddress())
		s.Require().NoError(err)
	}))

	s.Require().True(s.Run("Deploy SP1 ICS07 contract", func() {
		var verfierAddress string
		if prover == testvalues.EnvValueSp1Prover_Mock {
			verfierAddress = s.contractAddresses.VerifierMock
		} else {
			switch proofType {
			case types.ProofTypeGroth16:
				verfierAddress = s.contractAddresses.VerifierGroth16
			case types.ProofTypePlonk:
				verfierAddress = s.contractAddresses.VerifierPlonk
			default:
				s.Require().Fail("invalid proof type: %s", proofType)
			}
		}

		var createClientTxBz []byte
		s.Require().True(s.Run("Retrieve create client tx", func() {
			resp, err := s.RelayerClient.CreateClient(context.Background(), &relayertypes.CreateClientRequest{
				SrcChain: simd.Config().ChainID,
				DstChain: eth.ChainID.String(),
				Parameters: map[string]string{
					testvalues.ParameterKey_Sp1Verifier: verfierAddress,
					testvalues.ParameterKey_ZkAlgorithm: proofType.String(),
				},
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Tx)
			s.Require().Empty(resp.Address)

			createClientTxBz = resp.Tx
		}))

		s.Require().True(s.Run("Broadcast relay tx", func() {
			receipt, err := eth.BroadcastTx(ctx, s.EthRelayerSubmitter, 15_000_000, nil, createClientTxBz)
			s.Require().NoError(err)
			s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status, fmt.Sprintf("Tx failed: %+v", receipt))

			s.sp1Ics07Address = receipt.ContractAddress
			s.sp1Ics07Contract, err = sp1ics07tendermint.NewContract(s.sp1Ics07Address, eth.RPCClient)
			s.Require().NoError(err)
		}))
	}))

	s.Require().True(s.Run("Fund address with ERC20", func() {
		tx, err := s.erc20Contract.Transfer(s.GetTransactOpts(eth.Faucet, eth), crypto.PubkeyToAddress(s.key.PublicKey), testvalues.StartingERC20Balance)
		s.Require().NoError(err)

		_, err = eth.GetTxReciept(ctx, tx.Hash()) // wait for the tx to be mined
		s.Require().NoError(err)
	}))

	s.Require().True(s.Run("Create ethereum light client on Cosmos chain", func() {
		checksumHex := s.StoreLightClient(ctx, simd, s.SimdRelayerSubmitter)
		s.Require().NotEmpty(checksumHex)

		var createClientTxBodyBz []byte
		s.Require().NotEmpty(attestorResult.Addresses, "attestor addresses should be set")
		attestorAddress := attestorResult.Addresses[0]
		s.Require().True(s.Run("Retrieve create client tx", func() {
			resp, err := s.RelayerClient.CreateClient(context.Background(), &relayertypes.CreateClientRequest{
				SrcChain: eth.ChainID.String(),
				DstChain: simd.Config().ChainID,
				Parameters: map[string]string{
					testvalues.ParameterKey_ChecksumHex:       checksumHex,
					testvalues.ParameterKey_AttestorAddresses: ethcommon.HexToAddress(attestorAddress).Hex(),
					testvalues.ParameterKey_MinRequiredSigs:   strconv.Itoa(testvalues.DefaultMinRequiredSigs),
					testvalues.ParameterKey_height:            "0",
					testvalues.ParameterKey_timestamp:         "123456789",
				},
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Tx)
			s.Require().Empty(resp.Address)

			createClientTxBodyBz = resp.Tx
		}))

		err := s.wasmFixtureGenerator.AddInitialStateStep(createClientTxBodyBz)
		s.Require().NoError(err)

		s.Require().True(s.Run("Broadcast relay tx", func() {
			resp := s.MustBroadcastSdkTxBody(ctx, simd, s.SimdRelayerSubmitter, 20_000_000, createClientTxBodyBz)
			clientId, err := cosmos.GetEventValue(resp.Events, clienttypes.EventTypeCreateClient, clienttypes.AttributeKeyClientID)
			s.Require().NoError(err)
			s.Require().Equal(testvalues.FirstWasmClientID, clientId)
		}))
	}))

	s.Require().True(s.Run("Add client and counterparty on EVM", func() {
		counterpartyInfo := ics26router.IICS02ClientMsgsCounterpartyInfo{
			ClientId:     testvalues.FirstWasmClientID,
			MerklePrefix: [][]byte{[]byte(ibcexported.StoreKey), []byte("")},
		}
		tx, err := s.ics26Contract.AddClient(s.GetTransactOpts(s.deployer, eth), testvalues.CustomClientID, counterpartyInfo, s.sp1Ics07Address)
		s.Require().NoError(err)

		receipt, err := eth.GetTxReciept(ctx, tx.Hash())
		s.Require().NoError(err)

		event, err := e2esuite.GetEvmEvent(receipt, s.ics26Contract.ParseICS02ClientAdded)
		s.Require().NoError(err)
		s.Require().Equal(testvalues.CustomClientID, event.ClientId)
		s.Require().Equal(testvalues.FirstWasmClientID, event.CounterpartyInfo.ClientId)
	}))

	s.Require().True(s.Run("Register counterparty on Cosmos chain", func() {
		merklePathPrefix := [][]byte{[]byte("")}

		_, err := s.BroadcastMessages(ctx, simd, s.SimdRelayerSubmitter, 200_000, &clienttypesv2.MsgRegisterCounterparty{
			ClientId:                 testvalues.FirstWasmClientID,
			CounterpartyMerklePrefix: merklePathPrefix,
			CounterpartyClientId:     testvalues.CustomClientID,
			Signer:                   s.SimdRelayerSubmitter.FormattedAddress(),
		})
		s.Require().NoError(err)
	}))

	s.Require().True(s.Run("Generate the genesis fixtures", func() {
		if !s.solidityFixtureGenerator.Enabled {
			s.T().Skip("Skipping solidity fixture generation")
		}

		clientState, err := s.sp1Ics07Contract.ClientState(nil)
		s.Require().NoError(err)
		clientStateBz, err := s.sp1Ics07Contract.GetClientState(nil)
		s.Require().NoError(err)
		consensusStateHash, err := s.sp1Ics07Contract.GetConsensusStateHash(nil, clientState.LatestHeight.RevisionHeight)
		s.Require().NoError(err)
		updateClientVkey, err := s.sp1Ics07Contract.UPDATECLIENTPROGRAMVKEY(nil)
		s.Require().NoError(err)
		membershipVkey, err := s.sp1Ics07Contract.MEMBERSHIPPROGRAMVKEY(nil)
		s.Require().NoError(err)
		ucAndMembershipVkey, err := s.sp1Ics07Contract.UPDATECLIENTANDMEMBERSHIPPROGRAMVKEY(nil)
		s.Require().NoError(err)
		misbehaviourVkey, err := s.sp1Ics07Contract.MISBEHAVIOURPROGRAMVKEY(nil)
		s.Require().NoError(err)

		s.solidityFixtureGenerator.SetGenesisFixture(
			clientStateBz, consensusStateHash, updateClientVkey,
			membershipVkey, ucAndMembershipVkey, misbehaviourVkey,
		)
	}))
}

func (s *IbcAttestorTestSuite) Test_AggregatorStartUp() {
	ctx := context.Background()
	if os.Getenv(testvalues.EnvKeyEthLcOnCosmos) == testvalues.EthWasmTypeDummy {
		s.T().Skip("Skipping attestor e2e tests when ETH_LC_ON_COSMOS=dummy")
	}
	s.T().Setenv(testvalues.EnvKeyEthLcOnCosmos, testvalues.EthWasmTypeAttestorWasm)
	// Ensure the test uses the Optimism testnet type
	s.T().Setenv(testvalues.EnvKeyEthTestnetType, testvalues.EthTestnetTypeOptimism)
	s.AggregatorStartUpTest(ctx, attestor.ChainTypeEvm)
}

func (s *IbcAttestorTestSuite) AggregatorStartUpTest(ctx context.Context, chainType attestor.ChainType) {
	baseConfig := attestor.DefaultAttestorConfig()
	basePort, err := baseConfig.GetServerPort()
	s.Require().NoError(err)

	var attestorResult attestor.SetupResult
	s.Require().True(s.Run("Setup attestors", func() {
		attestorResult = attestor.SetupAttestors(ctx, s.T(), attestor.SetupParams{
			NumAttestors:         testvalues.NumAttestors,
			BasePort:             basePort,
			ConfigPathTemplate:   testvalues.EthAttestorConfigPathTemplate,
			KeystorePathTemplate: testvalues.AttestorKeystorePathTemplate,
			ChainType:            chainType,
			AdapterURL:           baseConfig.Adapter.URL,
			RouterAddress:        baseConfig.Adapter.RouterAddress,
		})
	}))

	s.T().Cleanup(func() {
		attestor.CleanupProcesses(s.T(), attestorResult.Processes)
	})
}

func (s *IbcAttestorTestSuite) Test_OptimismAttestToICS20PacketsOnEth() {
	ctx := context.Background()
	if os.Getenv(testvalues.EnvKeyEthLcOnCosmos) == testvalues.EthWasmTypeDummy {
		s.T().Skip("Skipping attestor e2e tests when ETH_LC_ON_COSMOS=dummy")
	}
	s.T().Setenv(testvalues.EnvKeyEthLcOnCosmos, testvalues.EthWasmTypeAttestorWasm)
	// Ensure the test uses the Optimism testnet type
	s.T().Setenv(testvalues.EnvKeyEthTestnetType, testvalues.EthTestnetTypeOptimism)
	proofType := types.GetEnvProofType()

	s.attestToICS20TransferNativeCosmosCoinsToEthereumNoReturn(ctx, proofType, big.NewInt(testvalues.TransferAmount), attestor.ChainTypeEvm)
}

// ICS20TransferNativeCosmosCoinsToEthereumAndBackTest tests the ICS20 transfer functionality
// by transferring native coins from a Cosmos chain to Ethereum and back
func (s *IbcAttestorTestSuite) attestToICS20TransferNativeCosmosCoinsToEthereumNoReturn(
	ctx context.Context,
	pt types.SupportedProofType,
	transferAmount *big.Int,
	chainType attestor.ChainType,
) {
	s.SetupSuite(ctx, pt, chainType)

	numOfTransfers := 1

	eth, simd := s.EthChains[0], s.CosmosChains[0]

	ics20Address := ethcommon.HexToAddress(s.contractAddresses.Ics20Transfer)
	erc20Address := ethcommon.HexToAddress(s.contractAddresses.Erc20)

	totalTransferAmount := new(big.Int).Mul(transferAmount, big.NewInt(int64(numOfTransfers)))
	ethereumUserAddress := crypto.PubkeyToAddress(s.key.PublicKey)
	cosmosUserWallet := s.CosmosUsers[0]
	cosmosUserAddress := cosmosUserWallet.FormattedAddress()

	ics20transferAbi, err := abi.JSON(strings.NewReader(ics20transfer.ContractABI))
	s.Require().NoError(err)

	s.Require().True(s.Run("Approve the ICS20Transfer.sol contract to spend the erc20 tokens", func() {
		tx, err := s.erc20Contract.Approve(s.GetTransactOpts(s.key, eth), ics20Address, totalTransferAmount)
		s.Require().NoError(err)

		receipt, err := eth.GetTxReciept(ctx, tx.Hash())
		s.Require().NoError(err)
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)

		allowance, err := s.erc20Contract.Allowance(nil, ethereumUserAddress, ics20Address)
		s.Require().NoError(err)
		s.Require().Equal(totalTransferAmount, allowance)
	}))

	var (
		sendPacket    ics26router.IICS26RouterMsgsPacket
		escrowAddress ethcommon.Address
		ethSendTxHash []byte
	)
	s.Require().True(s.Run(fmt.Sprintf("Send %d transfers on Ethereum", numOfTransfers), func() {
		timeout := uint64(time.Now().Add(30 * time.Minute).Unix())
		transferMulticall := make([][]byte, numOfTransfers)

		msgSendPacket := ics20transfer.IICS20TransferMsgsSendTransferMsg{
			Denom:            erc20Address,
			Amount:           transferAmount,
			Receiver:         cosmosUserAddress,
			TimeoutTimestamp: timeout,
			SourceClient:     testvalues.CustomClientID,
			Memo:             "",
		}

		encodedMsg, err := ics20transferAbi.Pack("sendTransfer", msgSendPacket)
		s.Require().NoError(err)
		for i := range numOfTransfers {
			transferMulticall[i] = encodedMsg
		}

		tx, err := s.ics20Contract.Multicall(s.GetTransactOpts(s.key, eth), transferMulticall)
		s.Require().NoError(err)
		receipt, err := eth.GetTxReciept(ctx, tx.Hash())
		ethSendTxHash = tx.Hash().Bytes()

		s.Require().NoError(err)
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)
		s.T().Logf("Multicall send %d transfers gas used: %d", numOfTransfers, receipt.GasUsed)

		sendPacketEvent, err := e2esuite.GetEvmEvent(receipt, s.ics26Contract.ParseSendPacket)
		s.Require().NoError(err)
		sendPacket = sendPacketEvent.Packet
		s.Require().Equal(uint64(1), sendPacket.Sequence)
		s.Require().Equal(timeout, sendPacket.TimeoutTimestamp)
		s.Require().Len(sendPacket.Payloads, 1)
		s.Require().Equal(transfertypes.PortID, sendPacket.Payloads[0].SourcePort)
		s.Require().Equal(testvalues.CustomClientID, sendPacket.SourceClient)
		s.Require().Equal(transfertypes.PortID, sendPacket.Payloads[0].DestPort)
		s.Require().Equal(testvalues.FirstWasmClientID, sendPacket.DestClient)
		s.Require().Equal(transfertypes.V1, sendPacket.Payloads[0].Version)
		s.Require().Equal(transfertypes.EncodingABI, sendPacket.Payloads[0].Encoding)

		s.True(s.Run("Verify balances on Ethereum", func() {
			// User balance on Ethereum
			userBalance, err := s.erc20Contract.BalanceOf(nil, ethereumUserAddress)
			s.Require().NoError(err)
			s.Require().Equal(new(big.Int).Sub(testvalues.StartingERC20Balance, totalTransferAmount), userBalance)

			// Get the escrow address
			escrowAddress, err = s.ics20Contract.GetEscrow(nil, testvalues.CustomClientID)
			s.Require().NoError(err)

			// ICS20 contract balance on Ethereum
			escrowBalance, err := s.erc20Contract.BalanceOf(nil, escrowAddress)
			s.Require().NoError(err)
			s.Require().Equal(totalTransferAmount, escrowBalance)
		}))
	}))

	var (
		denomOnCosmos transfertypes.Denom
		ackTxHash     []byte
	)
	s.Require().True(s.Run("Receive packets on Cosmos chain", func() {
		var relayTxBodyBz []byte
		s.Require().True(s.Run("Retrieve relay tx", func() {
			resp, err := s.RelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
				SrcChain:    eth.ChainID.String(),
				DstChain:    simd.Config().ChainID,
				SourceTxIds: [][]byte{ethSendTxHash},
				SrcClientId: testvalues.CustomClientID,
				DstClientId: testvalues.FirstWasmClientID,
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Tx)
			s.Require().Empty(resp.Address)

			relayTxBodyBz = resp.Tx
		}))

		s.Require().True(s.Run("Broadcast relay tx", func() {
			resp := s.MustBroadcastSdkTxBody(ctx, simd, s.SimdRelayerSubmitter, 20_000_000, relayTxBodyBz)

			var err error
			ackTxHash, err = hex.DecodeString(resp.TxHash)
			s.Require().NoError(err)
			s.Require().NotEmpty(ackTxHash)
		}))

		s.Require().True(s.Run("Verify balances on Cosmos chain", func() {
			denomOnCosmos = transfertypes.NewDenom(s.contractAddresses.Erc20, transfertypes.NewHop(transfertypes.PortID, testvalues.FirstWasmClientID))

			// User balance on Cosmos chain
			resp, err := e2esuite.GRPCQuery[banktypes.QueryBalanceResponse](ctx, simd, &banktypes.QueryBalanceRequest{
				Address: cosmosUserAddress,
				Denom:   denomOnCosmos.IBCDenom(),
			})
			s.Require().NoError(err)
			s.Require().NotNil(resp.Balance)
			s.Require().Equal(totalTransferAmount, resp.Balance.Amount.BigInt())
			s.Require().Equal(denomOnCosmos.IBCDenom(), resp.Balance.Denom)
		}))
	}))

	ics26Address := ethcommon.HexToAddress(s.contractAddresses.Ics26Router)
	s.Require().True(s.Run("Acknowledge packets on Ethereum", func() {
		var ackRelayTx []byte
		s.Require().True(s.Run("Retrieve relay tx", func() {
			resp, err := s.RelayerClient.RelayByTx(context.Background(), &relayertypes.RelayByTxRequest{
				SrcChain:    simd.Config().ChainID,
				DstChain:    eth.ChainID.String(),
				SourceTxIds: [][]byte{ackTxHash},
				SrcClientId: testvalues.FirstWasmClientID,
				DstClientId: testvalues.CustomClientID,
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Tx)
			s.Require().Equal(resp.Address, ics26Address.String())

			ackRelayTx = resp.Tx
		}))

		s.Require().True(s.Run("Submit relay tx", func() {
			receipt, err := eth.BroadcastTx(ctx, s.EthRelayerSubmitter, 15_000_000, &ics26Address, ackRelayTx)
			s.Require().NoError(err)
			s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status, fmt.Sprintf("Tx failed: %+v", receipt))
			s.T().Logf("Ack %d packets gas used: %d", numOfTransfers, receipt.GasUsed)

			// Verify the ack packet event exists
			_, err = e2esuite.GetEvmEvent(receipt, s.ics26Contract.ParseAckPacket)
			s.Require().NoError(err)
		}))

		s.Require().True(s.Run("Verify final balances on Ethereum", func() {
			// User balance on Ethereum should remain the same (tokens were transferred)
			userBalance, err := s.erc20Contract.BalanceOf(nil, ethereumUserAddress)
			s.Require().NoError(err)
			s.Require().Equal(new(big.Int).Sub(testvalues.StartingERC20Balance, totalTransferAmount), userBalance)

			// ICS20 contract balance on Ethereum should still hold the escrowed tokens
			escrowBalance, err := s.erc20Contract.BalanceOf(nil, escrowAddress)
			s.Require().NoError(err)
			s.Require().Equal(totalTransferAmount, escrowBalance)
		}))
	}))
}
