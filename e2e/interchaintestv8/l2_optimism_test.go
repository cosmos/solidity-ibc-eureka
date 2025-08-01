package main

import (
	"context"
	"crypto/ecdsa"
	"encoding/hex"
	"fmt"
	"math/big"
	"os"
	"strings"
	"testing"
	"time"

	"github.com/ethereum-optimism/optimism/op-service/client"
	"github.com/ethereum-optimism/optimism/op-service/sources"
	"github.com/stretchr/testify/suite"

	ethcommon "github.com/ethereum/go-ethereum/common"
	ethtypes "github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/crypto"
	"github.com/ethereum/go-ethereum/ethclient"

	"github.com/strangelove-ventures/interchaintest/v8/ibc"

	clienttypes "github.com/cosmos/ibc-go/v10/modules/core/02-client/types"
	clienttypesv2 "github.com/cosmos/ibc-go/v10/modules/core/02-client/v2/types"
	ibcexported "github.com/cosmos/ibc-go/v10/modules/core/exported"

	transfertypes "github.com/cosmos/ibc-go/v10/modules/apps/transfer/types"
	"github.com/cosmos/solidity-ibc-eureka/packages/go-abigen/ics20transfer"
	"github.com/cosmos/solidity-ibc-eureka/packages/go-abigen/ics26router"
	"github.com/cosmos/solidity-ibc-eureka/packages/go-abigen/sp1ics07tendermint"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/chainconfig"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/cosmos"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/e2esuite"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/ethereum"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/relayer"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/erc20"
	relayertypes "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/relayer"
)

// OptimismTestSuite is a struct that holds the test suite for Optimism L2 chain with IBC deployment.
type OptimismTestSuite struct {
	e2esuite.TestSuite

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

	OptimismChain chainconfig.KurtosisOptimismChain

	solidityFixtureGenerator *types.SolidityFixtureGenerator
	wasmFixtureGenerator     *types.WasmFixtureGenerator
}

// TestWithOptimismTestSuite is the boilerplate code that allows the test suite to be run
func TestWithOptimismTestSuite(t *testing.T) {
	suite.Run(t, new(OptimismTestSuite))
}

// SetupSuite sets up the optimism chain and deploys IBC contracts
func (s *OptimismTestSuite) SetupSuite(ctx context.Context, proofType types.SupportedProofType) {
	os.Setenv(testvalues.EnvKeyEthTestnetType, testvalues.EthTestnetTypeOptimism)

	s.TestSuite.SetupSuite(ctx)

	s.T().Logf("Setting up the Optimism test suite with proof type: %s", proofType.String())
	eth, simd := s.EthChain, s.CosmosChains[0]

	var prover string
	s.Require().True(s.Run("Set up environment", func() {
		err := os.Chdir("../..")
		s.Require().NoError(err)

		// Wait for ETH node to be ready before attempting to create users
		s.T().Logf("Waiting for ETH node to be ready...")
		err = eth.WaitForNodeReady(context.Background(), time.Minute*3)
		s.Require().NoError(err, "ETH node failed to become ready within timeout")
		s.T().Logf("ETH node is ready for transactions")

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

		if os.Getenv(testvalues.EnvKeyRustLog) == "" {
			os.Setenv(testvalues.EnvKeyRustLog, testvalues.EnvValueRustLog_Info)
		}
		os.Setenv(testvalues.EnvKeyEthRPC, eth.RPC)
		os.Setenv(testvalues.EnvKeyTendermintRPC, simd.GetHostRPCAddress())
		os.Setenv(testvalues.EnvKeySp1Prover, prover)
		os.Setenv(testvalues.EnvKeyOperatorPrivateKey, hex.EncodeToString(crypto.FromECDSA(operatorKey)))
	}))

	s.wasmFixtureGenerator = types.NewWasmFixtureGenerator(&s.Suite)
	s.solidityFixtureGenerator = types.NewSolidityFixtureGenerator()

	s.Require().True(s.Run("Deploy IBC contracts on Optimism", func() {
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

	var relayerProcess *os.Process
	s.Require().True(s.Run("Start Relayer", func() {
		// Optimism is an L2, so no beacon API client
		beaconAPI := ""

		sp1Config := relayer.SP1ProverConfig{
			Type:           prover,
			PrivateCluster: os.Getenv(testvalues.EnvKeyNetworkPrivateCluster) == testvalues.EnvValueSp1Prover_PrivateCluster,
		}

		config := relayer.NewConfig(relayer.CreateEthCosmosModules(
			relayer.EthCosmosConfigInfo{
				EthChainID:     eth.ChainID.String(),
				CosmosChainID:  simd.Config().ChainID,
				TmRPC:          simd.GetHostRPCAddress(),
				ICS26Address:   s.contractAddresses.Ics26Router,
				EthRPC:         eth.RPC,
				BeaconAPI:      beaconAPI,
				SP1Config:      sp1Config,
				SignerAddress:  s.SimdRelayerSubmitter.FormattedAddress(),
				MockWasmClient: true, // Optimism is L2, so we use mock wasm client
			}),
		)

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

	s.Require().True(s.Run("Deploy SP1 ICS07 contract on Optimism", func() {
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
		checksumHex := s.StoreEthereumLightClient(ctx, simd, s.SimdRelayerSubmitter)
		s.Require().NotEmpty(checksumHex)

		var createClientTxBodyBz []byte
		s.Require().True(s.Run("Retrieve create client tx", func() {
			resp, err := s.RelayerClient.CreateClient(context.Background(), &relayertypes.CreateClientRequest{
				SrcChain: eth.ChainID.String(),
				DstChain: simd.Config().ChainID,
				Parameters: map[string]string{
					testvalues.ParameterKey_ChecksumHex: checksumHex,
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

	s.Require().True(s.Run("Add client and counterparty on Optimism", func() {
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
}

func (s *OptimismTestSuite) Test_Deploy() {
	ctx := context.Background()
	proofType := types.GetEnvProofType()
	s.DeployTest(ctx, proofType)
}

func (s *OptimismTestSuite) DeployTest(ctx context.Context, proofType types.SupportedProofType) {
	s.SetupSuite(ctx, proofType)
	s.OptimismChain = s.TestSuite.OptimismChain

	eth, simd := s.EthChain, s.CosmosChains[0]

	s.Require().True(s.Run("Verify Optimism chain properties", func() {

		consensusClient, err := ethclient.Dial(s.OptimismChain.ConsensusRPC)
		s.Require().NoError(err)
		baseClient := client.NewBaseRPCClient(consensusClient.Client())
		rollupClient := sources.NewRollupClient(baseClient)

		rollupConfig, err := rollupClient.RollupConfig(ctx)
		s.Require().NoError(err)
		s.T().Logf("Rollup config: %+v", rollupConfig)

		blockNumber, err := eth.RPCClient.BlockNumber(ctx)
		s.Require().NoError(err)
		s.T().Logf("Latest block number: %d", blockNumber)

		s.Require().NotZero(eth.ChainID)
		s.T().Logf("Chain ID: %s", eth.ChainID.String())
	}))

	// NOTE: This is close to working but needs the rest of the
	// relayer setup.

	s.Require().True(s.Run("Send packet to ETH", func() {
		ics20Address := ethcommon.HexToAddress(s.contractAddresses.Ics20Transfer)
		erc20Address := ethcommon.HexToAddress(s.contractAddresses.Erc20)

		transferAmount := big.NewInt(5)
		ethereumUserAddress := crypto.PubkeyToAddress(s.key.PublicKey)
		cosmosUserWallet := s.CosmosUsers[0]
		cosmosUserAddress := cosmosUserWallet.FormattedAddress()

		s.Require().True(s.Run("Approve the ICS20Transfer.sol contract to spend the erc20 tokens", func() {
			tx, err := s.erc20Contract.Approve(s.GetTransactOpts(s.key, eth), ics20Address, transferAmount)
			s.Require().NoError(err)

			receipt, err := eth.GetTxReciept(ctx, tx.Hash())
			s.Require().NoError(err)
			s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)

			allowance, err := s.erc20Contract.Allowance(nil, ethereumUserAddress, ics20Address)
			s.Require().NoError(err)
			s.Require().Equal(transferAmount, allowance)
		}))

		var (
			sendPacket    ics26router.IICS26RouterMsgsPacket
			escrowAddress ethcommon.Address
		)
		s.Require().True(s.Run("Send ERC20 tokens on Ethereum", func() {
			timeout := uint64(time.Now().Add(30 * time.Minute).Unix())

			msgSendPacket := ics20transfer.IICS20TransferMsgsSendTransferMsg{
				Denom:            erc20Address,
				Amount:           transferAmount,
				Receiver:         cosmosUserAddress,
				TimeoutTimestamp: timeout,
				SourceClient:     testvalues.CustomClientID,
				Memo:             "create-voucher-cosmos",
			}

			tx, err := s.ics20Contract.SendTransfer(s.GetTransactOpts(s.key, eth), msgSendPacket)
			s.Require().NoError(err)
			receipt, err := eth.GetTxReciept(ctx, tx.Hash())
			s.Require().NoError(err)
			s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)

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
				userBalance, err := s.erc20Contract.BalanceOf(nil, ethereumUserAddress)
				s.Require().NoError(err)
				s.Require().Equal(new(big.Int).Sub(testvalues.StartingERC20Balance, transferAmount), userBalance)

				escrowAddress, err = s.ics20Contract.GetEscrow(nil, testvalues.CustomClientID)
				s.Require().NoError(err)

				escrowBalance, err := s.erc20Contract.BalanceOf(nil, escrowAddress)
				s.Require().NoError(err)
				s.Require().Equal(transferAmount, escrowBalance)
			}))
		}))
	}))

	s.Require().True(s.Run("Verify ICS02 Client on Optimism", func() {
		clientAddress, err := s.ics26Contract.GetClient(nil, testvalues.CustomClientID)
		s.Require().NoError(err)
		s.Require().Equal(s.sp1Ics07Address, clientAddress)

		counterpartyInfo, err := s.ics26Contract.GetCounterparty(nil, testvalues.CustomClientID)
		s.Require().NoError(err)
		s.Require().Equal(testvalues.FirstWasmClientID, counterpartyInfo.ClientId)
	}))

	s.Require().True(s.Run("Verify ICS26 Router on Optimism", func() {
		transferAddress, err := s.ics26Contract.GetIBCApp(nil, transfertypes.PortID)
		s.Require().NoError(err)
		s.Require().Equal(s.contractAddresses.Ics20Transfer, strings.ToLower(transferAddress.Hex()))
	}))

	s.Require().True(s.Run("Verify ERC20 Genesis on Optimism", func() {
		userBalance, err := s.erc20Contract.BalanceOf(nil, crypto.PubkeyToAddress(s.key.PublicKey))
		s.Require().NoError(err)
		s.Require().Equal(testvalues.StartingERC20Balance, userBalance)
	}))

	s.Require().True(s.Run("Verify SP1 Client on Optimism", func() {
		clientState, err := s.sp1Ics07Contract.ClientState(nil)
		s.Require().NoError(err)

		stakingParams, err := simd.StakingQueryParams(ctx)
		s.Require().NoError(err)

		s.Require().Equal(simd.Config().ChainID, clientState.ChainId)
		s.Require().Equal(uint8(testvalues.DefaultTrustLevel.Numerator), clientState.TrustLevel.Numerator)
		s.Require().Equal(uint8(testvalues.DefaultTrustLevel.Denominator), clientState.TrustLevel.Denominator)
		s.Require().Equal(uint32(testvalues.DefaultTrustPeriod), clientState.TrustingPeriod)
		s.Require().Equal(uint32(stakingParams.UnbondingTime.Seconds()), clientState.UnbondingPeriod)
		s.Require().False(clientState.IsFrozen)
		s.Require().Equal(uint64(1), clientState.LatestHeight.RevisionNumber)
		s.Require().Greater(clientState.LatestHeight.RevisionHeight, uint64(0))
	}))

	s.Require().True(s.Run("Verify Optimism light client on Cosmos", func() {
		_, err := e2esuite.GRPCQuery[clienttypes.QueryClientStateResponse](ctx, simd, &clienttypes.QueryClientStateRequest{
			ClientId: testvalues.FirstWasmClientID,
		})
		s.Require().NoError(err)

		counterpartyInfoResp, err := e2esuite.GRPCQuery[clienttypesv2.QueryCounterpartyInfoResponse](ctx, simd, &clienttypesv2.QueryCounterpartyInfoRequest{
			ClientId: testvalues.FirstWasmClientID,
		})
		s.Require().NoError(err)
		s.Require().Equal(testvalues.CustomClientID, counterpartyInfoResp.CounterpartyInfo.ClientId)
	}))

	s.Require().True(s.Run("Verify Cosmos to Optimism Relayer Info", func() {
		info, err := s.RelayerClient.Info(context.Background(), &relayertypes.InfoRequest{
			SrcChain: simd.Config().ChainID,
			DstChain: eth.ChainID.String(),
		})
		s.Require().NoError(err)
		s.Require().NotNil(info)
		s.Require().Equal(simd.Config().ChainID, info.SourceChain.ChainId)
		s.Require().Equal(eth.ChainID.String(), info.TargetChain.ChainId)
	}))

	s.Require().True(s.Run("Verify Optimism to Cosmos Relayer Info", func() {
		info, err := s.RelayerClient.Info(context.Background(), &relayertypes.InfoRequest{
			SrcChain: eth.ChainID.String(),
			DstChain: simd.Config().ChainID,
		})
		s.Require().NoError(err)
		s.Require().NotNil(info)
		s.Require().Equal(eth.ChainID.String(), info.SourceChain.ChainId)
		s.Require().Equal(simd.Config().ChainID, info.TargetChain.ChainId)
	}))
}
