package main

import (
	"context"
	"crypto/ecdsa"
	"encoding/hex"
	"math/big"
	"os"
	"testing"
	"time"

	"github.com/stretchr/testify/suite"

	ethcommon "github.com/ethereum/go-ethereum/common"
	ethtypes "github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/crypto"

	sdkmath "cosmossdk.io/math"

	sdk "github.com/cosmos/cosmos-sdk/types"
	authtypes "github.com/cosmos/cosmos-sdk/x/auth/types"
	banktypes "github.com/cosmos/cosmos-sdk/x/bank/types"
	govtypes "github.com/cosmos/cosmos-sdk/x/gov/types"

	gmptypes "github.com/cosmos/ibc-go/v10/modules/apps/27-gmp/types"
	clienttypes "github.com/cosmos/ibc-go/v10/modules/core/02-client/types"
	clienttypesv2 "github.com/cosmos/ibc-go/v10/modules/core/02-client/v2/types"
	ibcexported "github.com/cosmos/ibc-go/v10/modules/core/exported"

	interchaintest "github.com/cosmos/interchaintest/v10"
	interchaintestcosmos "github.com/cosmos/interchaintest/v10/chain/cosmos"
	"github.com/cosmos/interchaintest/v10/ibc"

	"github.com/cosmos/solidity-ibc-eureka/packages/go-abigen/ics26router"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/chainconfig"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/cosmos"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/e2esuite"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/ethereum"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/relayer"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/cosmosiftconstructor"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/evmift"
	relayertypes "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/relayer"
	ifttypes "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/wfchain/ift"
	tokenfactorytypes "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/wfchain/tokenfactory"
)

const (
	cosmosIFTDenom            = "testift"
	cosmosIFTModuleName       = "ift"
	iftSendCallConstructorEVM = "evm"
)

// CosmosEthereumIFTTestSuite tests IFT transfers between wfchain and Ethereum
type CosmosEthereumIFTTestSuite struct {
	e2esuite.TestSuite

	// Cosmos chain
	Wfchain *interchaintestcosmos.CosmosChain

	// Ethereum contracts
	contractAddresses ethereum.DeployedContracts

	// Keys
	ethDeployer         *ecdsa.PrivateKey
	ethUser             *ecdsa.PrivateKey
	EthRelayerSubmitter *ecdsa.PrivateKey

	// Cosmos wallets
	CosmosRelayerSubmitter ibc.Wallet
	CosmosUser             ibc.Wallet

	// Relayer
	RelayerClient relayertypes.RelayerServiceClient

	// Fixture generators
	solidityFixtureGenerator *types.SolidityFixtureGenerator
	wasmFixtureGenerator     *types.WasmFixtureGenerator
}

func TestWithCosmosEthereumIFTTestSuite(t *testing.T) {
	suite.Run(t, new(CosmosEthereumIFTTestSuite))
}

func (s *CosmosEthereumIFTTestSuite) SetupSuite(ctx context.Context, proofType types.SupportedProofType) {
	// Use wfchain as the Cosmos chain
	chainconfig.DefaultChainSpecs = []*interchaintest.ChainSpec{
		chainconfig.WfchainChainSpec("wfchain-1", "wfchain-1"),
	}

	// Set up Ethereum chain (use Anvil for local testing)
	os.Setenv(testvalues.EnvKeyEthTestnetType, testvalues.EthTestnetTypeAnvil)

	s.TestSuite.SetupSuite(ctx)

	eth := s.Eth.Chains[0]
	s.Wfchain = s.Cosmos.Chains[0]

	s.T().Logf("Setting up Cosmos-Ethereum IFT test suite with proof type: %s", proofType.String())

	var prover string
	s.Require().True(s.Run("Set up environment", func() {
		err := os.Chdir("../..")
		s.Require().NoError(err)

		s.ethUser, err = eth.CreateAndFundUser()
		s.Require().NoError(err)

		s.EthRelayerSubmitter, err = eth.CreateAndFundUser()
		s.Require().NoError(err)

		operatorKey, err := eth.CreateAndFundUser()
		s.Require().NoError(err)

		s.ethDeployer, err = eth.CreateAndFundUser()
		s.Require().NoError(err)

		s.CosmosRelayerSubmitter = s.CreateAndFundCosmosUser(ctx, s.Wfchain)
		s.CosmosUser = s.CreateAndFundCosmosUser(ctx, s.Wfchain)

		prover = os.Getenv(testvalues.EnvKeySp1Prover)
		switch prover {
		case "", testvalues.EnvValueSp1Prover_Mock:
			s.T().Logf("Using mock prover")
			prover = testvalues.EnvValueSp1Prover_Mock
			os.Setenv(testvalues.EnvKeySp1Prover, testvalues.EnvValueSp1Prover_Mock)
			os.Setenv(testvalues.EnvKeyVerifier, testvalues.EnvValueVerifier_Mock)
		case testvalues.EnvValueSp1Prover_Network:
			s.Require().NotEmpty(os.Getenv(testvalues.EnvKeyNetworkPrivateKey))
		default:
			s.Require().Fail("invalid prover type: %s", prover)
		}

		if os.Getenv(testvalues.EnvKeyRustLog) == "" {
			os.Setenv(testvalues.EnvKeyRustLog, testvalues.EnvValueRustLog_Info)
		}
		os.Setenv(testvalues.EnvKeyEthRPC, eth.RPC)
		os.Setenv(testvalues.EnvKeyTendermintRPC, s.Wfchain.GetHostRPCAddress())
		os.Setenv(testvalues.EnvKeySp1Prover, prover)
		os.Setenv(testvalues.EnvKeyOperatorPrivateKey, hex.EncodeToString(crypto.FromECDSA(operatorKey)))
	}))

	s.wasmFixtureGenerator = types.NewWasmFixtureGenerator(&s.Suite)
	s.solidityFixtureGenerator = types.NewSolidityFixtureGenerator()

	s.Require().True(s.Run("Deploy IBC contracts", func() {
		stdout, err := eth.ForgeScript(s.ethDeployer, testvalues.E2EDeployScriptPath)
		s.Require().NoError(err)

		s.contractAddresses, err = ethereum.GetEthContractsFromDeployOutput(string(stdout))
		s.Require().NoError(err)
	}))

	var relayerProcess *os.Process
	s.Require().True(s.Run("Start Relayer", func() {
		beaconAPI := ""
		if eth.BeaconAPIClient != nil {
			beaconAPI = eth.BeaconAPIClient.GetBeaconAPIURL()
		}

		sp1Config := relayer.SP1ProverConfig{
			Type:           prover,
			PrivateCluster: os.Getenv(testvalues.EnvKeyNetworkPrivateCluster) == testvalues.EnvValueSp1Prover_PrivateCluster,
		}

		mockWasmClient := os.Getenv(testvalues.EnvKeyEthTestnetType) == testvalues.EthTestnetTypeAnvil
		config := relayer.NewConfigBuilder().
			EthToCosmos(relayer.EthToCosmosParams{
				EthChainID:    eth.ChainID.String(),
				CosmosChainID: s.Wfchain.Config().ChainID,
				TmRPC:         s.Wfchain.GetHostRPCAddress(),
				ICS26Address:  s.contractAddresses.Ics26Router,
				EthRPC:        eth.RPC,
				BeaconAPI:     beaconAPI,
				SignerAddress: s.CosmosRelayerSubmitter.FormattedAddress(),
				MockClient:    mockWasmClient,
			}).
			CosmosToEthSP1(relayer.CosmosToEthSP1Params{
				CosmosChainID: s.Wfchain.Config().ChainID,
				EthChainID:    eth.ChainID.String(),
				TmRPC:         s.Wfchain.GetHostRPCAddress(),
				ICS26Address:  s.contractAddresses.Ics26Router,
				EthRPC:        eth.RPC,
				Prover:        sp1Config,
			}).
			Build()

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
			_ = relayerProcess.Kill()
		}
	})

	s.Require().True(s.Run("Create Relayer Client", func() {
		var err error
		s.RelayerClient, err = relayer.GetGRPCClient(relayer.DefaultRelayerGRPCAddress())
		s.Require().NoError(err)
	}))
}

func (s *CosmosEthereumIFTTestSuite) getIFTModuleAddress() string {
	iftAddr := authtypes.NewModuleAddress(cosmosIFTModuleName)
	bech32Addr, err := sdk.Bech32ifyAddressBytes(s.Wfchain.Config().Bech32Prefix, iftAddr)
	s.Require().NoError(err)
	return bech32Addr
}

func (s *CosmosEthereumIFTTestSuite) createTokenFactoryDenom(ctx context.Context, user ibc.Wallet, denom string) string {
	msg := &tokenfactorytypes.MsgCreateDenom{
		Sender: user.FormattedAddress(),
		Denom:  denom,
	}

	_, err := s.BroadcastMessages(ctx, s.Wfchain, user, 200_000, msg)
	s.Require().NoError(err)

	return denom
}

func (s *CosmosEthereumIFTTestSuite) mintTokensOnCosmos(ctx context.Context, user ibc.Wallet, denom string, amount sdkmath.Int, recipient string) {
	msg := &tokenfactorytypes.MsgMint{
		From:    user.FormattedAddress(),
		Address: recipient,
		Amount:  sdk.Coin{Denom: denom, Amount: amount},
	}

	_, err := s.BroadcastMessages(ctx, s.Wfchain, user, 200_000, msg)
	s.Require().NoError(err)
}

func (s *CosmosEthereumIFTTestSuite) registerIFTBridgeOnCosmos(ctx context.Context, user ibc.Wallet, denom, clientId, counterpartyIftAddr, constructor string) {
	govModuleAddr, err := s.Wfchain.AuthQueryModuleAddress(ctx, govtypes.ModuleName)
	s.Require().NoError(err)

	msg := &ifttypes.MsgRegisterIFTBridge{
		Signer:                 govModuleAddr,
		Denom:                  denom,
		ClientId:               clientId,
		CounterpartyIftAddress: counterpartyIftAddr,
		IftSendCallConstructor: constructor,
	}

	err = s.ExecuteGovV1Proposal(ctx, msg, s.Wfchain, user)
	s.Require().NoError(err)
}

func (s *CosmosEthereumIFTTestSuite) iftTransferFromCosmos(ctx context.Context, user ibc.Wallet, denom, clientId, receiver string, amount sdkmath.Int, timeoutTimestamp uint64) string {
	msg := &ifttypes.MsgIFTTransfer{
		Signer:           user.FormattedAddress(),
		Denom:            denom,
		ClientId:         clientId,
		Receiver:         receiver,
		Amount:           amount,
		TimeoutTimestamp: timeoutTimestamp,
	}

	resp, err := s.BroadcastMessages(ctx, s.Wfchain, user, 200_000, msg)
	s.Require().NoError(err)

	return resp.TxHash
}

func (s *CosmosEthereumIFTTestSuite) queryCosmosBalance(ctx context.Context, address, denom string) sdkmath.Int {
	resp, err := e2esuite.GRPCQuery[banktypes.QueryBalanceResponse](ctx, s.Wfchain, &banktypes.QueryBalanceRequest{
		Address: address,
		Denom:   denom,
	})
	s.Require().NoError(err)
	return resp.Balance.Amount
}

func (s *CosmosEthereumIFTTestSuite) queryPendingTransferOnCosmos(ctx context.Context, denom, clientId string, sequence uint64) (*ifttypes.PendingTransfer, error) {
	resp, err := e2esuite.GRPCQuery[ifttypes.QueryPendingTransferResponse](ctx, s.Wfchain, &ifttypes.QueryPendingTransferRequest{
		Denom:    denom,
		ClientId: clientId,
		Sequence: sequence,
	})
	if err != nil {
		return nil, err
	}
	return &resp.PendingTransfer, nil
}

func (s *CosmosEthereumIFTTestSuite) Test_Deploy() {
	ctx := context.Background()
	s.SetupSuite(ctx, types.ProofTypeGroth16)

	s.Require().True(s.Run("Verify Cosmos chain is running", func() {
		height, err := s.Wfchain.Height(ctx)
		s.Require().NoError(err)
		s.Require().Greater(height, int64(0))
		s.T().Logf("Wfchain height: %d", height)
	}))

	s.Require().True(s.Run("Verify Ethereum chain is running", func() {
		blockNum, err := s.Eth.Chains[0].RPCClient.BlockNumber(ctx)
		s.Require().NoError(err)
		s.Require().Greater(blockNum, uint64(0))
		s.T().Logf("Ethereum block: %d", blockNum)
	}))
}

type iftTestContext struct {
	ethIFTAddress   ethcommon.Address
	tmClientID      string
	wasmClientID    string
	ics26Address    ethcommon.Address
	sp1Ics07Address ethcommon.Address
	cosmosDenom     string
}

func (s *CosmosEthereumIFTTestSuite) setupIFTInfrastructure(ctx context.Context) iftTestContext {
	eth := s.Eth.Chains[0]
	tc := iftTestContext{
		tmClientID:   testvalues.CustomClientID,
		wasmClientID: testvalues.FirstWasmClientID,
		ics26Address: ethcommon.HexToAddress(s.contractAddresses.Ics26Router),
	}

	s.Require().True(s.Run("Setup light clients", func() {
		s.Require().True(s.Run("Create Tendermint light client on Ethereum", func() {
			var createClientTxBodyBz []byte
			s.Require().True(s.Run("Retrieve create client tx", func() {
				resp, err := s.RelayerClient.CreateClient(ctx, &relayertypes.CreateClientRequest{
					SrcChain: s.Wfchain.Config().ChainID,
					DstChain: eth.ChainID.String(),
					Parameters: map[string]string{
						testvalues.ParameterKey_Sp1Verifier: s.contractAddresses.VerifierMock,
						testvalues.ParameterKey_ZkAlgorithm: types.ProofTypeGroth16.String(),
					},
				})
				s.Require().NoError(err)
				s.Require().NotEmpty(resp.Tx)
				createClientTxBodyBz = resp.Tx
			}))

			s.Require().True(s.Run("Broadcast create client tx", func() {
				receipt, err := eth.BroadcastTx(ctx, s.EthRelayerSubmitter, 15_000_000, nil, createClientTxBodyBz)
				s.Require().NoError(err)
				s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)
				tc.sp1Ics07Address = receipt.ContractAddress
			}))
		}))

		s.Require().True(s.Run("Create Ethereum light client on Cosmos", func() {
			checksumHex := s.StoreLightClient(ctx, s.Wfchain, s.CosmosRelayerSubmitter)
			s.Require().NotEmpty(checksumHex)

			var createClientTxBodyBz []byte
			s.Require().True(s.Run("Retrieve create client tx", func() {
				resp, err := s.RelayerClient.CreateClient(ctx, &relayertypes.CreateClientRequest{
					SrcChain: eth.ChainID.String(),
					DstChain: s.Wfchain.Config().ChainID,
					Parameters: map[string]string{
						testvalues.ParameterKey_ChecksumHex: checksumHex,
					},
				})
				s.Require().NoError(err)
				s.Require().NotEmpty(resp.Tx)
				createClientTxBodyBz = resp.Tx
			}))

			s.Require().True(s.Run("Broadcast relay tx", func() {
				resp := s.MustBroadcastSdkTxBody(ctx, s.Wfchain, s.CosmosRelayerSubmitter, 20_000_000, createClientTxBodyBz)
				clientId, err := cosmos.GetEventValue(resp.Events, clienttypes.EventTypeCreateClient, clienttypes.AttributeKeyClientID)
				s.Require().NoError(err)
				s.Require().Equal(tc.wasmClientID, clientId)
			}))
		}))

		s.Require().True(s.Run("Add client and counterparty on Ethereum", func() {
			ics26Contract, err := ics26router.NewContract(tc.ics26Address, eth.RPCClient)
			s.Require().NoError(err)

			counterpartyInfo := ics26router.IICS02ClientMsgsCounterpartyInfo{
				ClientId:     tc.wasmClientID,
				MerklePrefix: [][]byte{[]byte(ibcexported.StoreKey), []byte("")},
			}

			txOpts, err := eth.GetTransactOpts(s.ethDeployer)
			s.Require().NoError(err)

			tx, err := ics26Contract.AddClient(txOpts, tc.tmClientID, counterpartyInfo, tc.sp1Ics07Address)
			s.Require().NoError(err)

			receipt, err := eth.GetTxReciept(ctx, tx.Hash())
			s.Require().NoError(err)
			s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)
		}))

		s.Require().True(s.Run("Register counterparty on Cosmos", func() {
			merklePathPrefix := [][]byte{[]byte("")}

			_, err := s.BroadcastMessages(ctx, s.Wfchain, s.CosmosRelayerSubmitter, 200_000, &clienttypesv2.MsgRegisterCounterparty{
				ClientId:                 tc.wasmClientID,
				CounterpartyMerklePrefix: merklePathPrefix,
				CounterpartyClientId:     tc.tmClientID,
				Signer:                   s.CosmosRelayerSubmitter.FormattedAddress(),
			})
			s.Require().NoError(err)
		}))
	}))

	var cosmosIftConstructorAddr ethcommon.Address
	s.Require().True(s.Run("Setup IFT bridges", func() {
		s.Require().True(s.Run("Verify IFT contract on Ethereum", func() {
			s.Require().NotEmpty(s.contractAddresses.Ift)
			tc.ethIFTAddress = ethcommon.HexToAddress(s.contractAddresses.Ift)
		}))

		s.Require().True(s.Run("Create denom on Cosmos", func() {
			tc.cosmosDenom = s.createTokenFactoryDenom(ctx, s.CosmosRelayerSubmitter, cosmosIFTDenom)
		}))

		s.Require().True(s.Run("Query and deploy CosmosIFTSendCallConstructor with correct ICS27 account", func() {
			// Query the correct ICS27 account address for the Ethereum IFT contract
			// The ICS27 account is derived from: wasm client ID + sender (IFT address) + salt (empty)
			ethIftAddressChecksummed := ethcommon.HexToAddress(s.contractAddresses.Ift).Hex()
			resp, err := e2esuite.GRPCQuery[gmptypes.QueryAccountAddressResponse](ctx, s.Wfchain, &gmptypes.QueryAccountAddressRequest{
				ClientId: tc.wasmClientID,
				Sender:   ethIftAddressChecksummed,
				Salt:     "",
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.AccountAddress)
			s.T().Logf("ICS27 account address for IFT: %s", resp.AccountAddress)

			// Deploy new CosmosIFTSendCallConstructor with correct ICS27 account address
			txOpts, err := eth.GetTransactOpts(s.ethDeployer)
			s.Require().NoError(err)

			addr, deployTx, _, err := cosmosiftconstructor.DeployContract(
				txOpts,
				eth.RPCClient,
				testvalues.WfchainIFTMintTypeURL,
				cosmosIFTDenom,
				resp.AccountAddress,
			)
			s.Require().NoError(err)

			receipt, err := eth.GetTxReciept(ctx, deployTx.Hash())
			s.Require().NoError(err)
			s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)

			cosmosIftConstructorAddr = addr
			s.T().Logf("Deployed CosmosIFTSendCallConstructor at: %s", cosmosIftConstructorAddr.Hex())
		}))

		s.Require().True(s.Run("Register IFT bridge on Ethereum", func() {
			cosmosIFTModuleAddr := s.getIFTModuleAddress()

			iftContract, err := evmift.NewContract(tc.ethIFTAddress, eth.RPCClient)
			s.Require().NoError(err)

			txOpts, err := eth.GetTransactOpts(s.ethDeployer)
			s.Require().NoError(err)

			tx, err := iftContract.RegisterIFTBridge(txOpts, tc.tmClientID, cosmosIFTModuleAddr, cosmosIftConstructorAddr)
			s.Require().NoError(err)

			receipt, err := eth.GetTxReciept(ctx, tx.Hash())
			s.Require().NoError(err)
			s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)
		}))

		s.Require().True(s.Run("Register IFT bridge on Cosmos", func() {
			s.registerIFTBridgeOnCosmos(
				ctx,
				s.CosmosRelayerSubmitter,
				tc.cosmosDenom,
				tc.wasmClientID,
				tc.ethIFTAddress.Hex(),
				iftSendCallConstructorEVM,
			)
		}))
	}))

	return tc
}

func (s *CosmosEthereumIFTTestSuite) Test_IFTTransfer_Roundtrip() {
	ctx := context.Background()
	s.SetupSuite(ctx, types.ProofTypeGroth16)

	eth := s.Eth.Chains[0]
	transferAmount := sdkmath.NewInt(1_000_000)

	tc := s.setupIFTInfrastructure(ctx)

	s.Require().True(s.Run("Mint tokens to user on Cosmos", func() {
		s.mintTokensOnCosmos(ctx, s.CosmosRelayerSubmitter, tc.cosmosDenom, transferAmount, s.CosmosUser.FormattedAddress())
		balance := s.queryCosmosBalance(ctx, s.CosmosUser.FormattedAddress(), tc.cosmosDenom)
		s.Require().True(balance.Equal(transferAmount))
	}))

	ethReceiverAddr := crypto.PubkeyToAddress(s.ethUser.PublicKey)
	var cosmosRecvTxHash []byte

	cosmosSequence := uint64(1)
	s.Require().True(s.Run("Transfer Cosmos to Ethereum", func() {
		var cosmosSendTxHash string
		s.Require().True(s.Run("Execute IFT transfer", func() {
			timeout := uint64(time.Now().Add(30 * time.Minute).Unix())
			cosmosSendTxHash = s.iftTransferFromCosmos(
				ctx,
				s.CosmosUser,
				tc.cosmosDenom,
				tc.wasmClientID,
				ethReceiverAddr.Hex(),
				transferAmount,
				timeout,
			)
			s.Require().NotEmpty(cosmosSendTxHash)

			balance := s.queryCosmosBalance(ctx, s.CosmosUser.FormattedAddress(), tc.cosmosDenom)
			s.Require().True(balance.IsZero())
		}))

		s.Require().True(s.Run("Verify pending transfer exists on Cosmos", func() {
			pending, err := s.queryPendingTransferOnCosmos(ctx, tc.cosmosDenom, tc.wasmClientID, cosmosSequence)
			s.Require().NoError(err)
			s.Require().Equal(s.CosmosUser.FormattedAddress(), pending.Sender)
			s.Require().True(pending.Amount.Equal(transferAmount))
		}))

		s.Require().True(s.Run("Relay packet to Ethereum", func() {
			sendTxHashBytes, err := hex.DecodeString(cosmosSendTxHash)
			s.Require().NoError(err)

			resp, err := s.RelayerClient.RelayByTx(ctx, &relayertypes.RelayByTxRequest{
				SrcChain:    s.Wfchain.Config().ChainID,
				DstChain:    eth.ChainID.String(),
				SourceTxIds: [][]byte{sendTxHashBytes},
				SrcClientId: tc.wasmClientID,
				DstClientId: tc.tmClientID,
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Tx)

			receipt, err := eth.BroadcastTx(ctx, s.EthRelayerSubmitter, 15_000_000, &tc.ics26Address, resp.Tx)
			s.Require().NoError(err)
			s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)
			cosmosRecvTxHash = receipt.TxHash.Bytes()
		}))

		s.Require().True(s.Run("Verify balance on Ethereum", func() {
			iftContract, err := evmift.NewContract(tc.ethIFTAddress, eth.RPCClient)
			s.Require().NoError(err)

			balance, err := iftContract.BalanceOf(nil, ethReceiverAddr)
			s.Require().NoError(err)
			s.Require().True(balance.Cmp(transferAmount.BigInt()) == 0)
		}))

		s.Require().True(s.Run("Relay ack to Cosmos", func() {
			resp, err := s.RelayerClient.RelayByTx(ctx, &relayertypes.RelayByTxRequest{
				SrcChain:    eth.ChainID.String(),
				DstChain:    s.Wfchain.Config().ChainID,
				SourceTxIds: [][]byte{cosmosRecvTxHash},
				SrcClientId: tc.tmClientID,
				DstClientId: tc.wasmClientID,
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Tx)

			_ = s.MustBroadcastSdkTxBody(ctx, s.Wfchain, s.CosmosRelayerSubmitter, 2_000_000, resp.Tx)
		}))

		s.Require().True(s.Run("Verify pending transfer cleared on Cosmos", func() {
			_, err := s.queryPendingTransferOnCosmos(ctx, tc.cosmosDenom, tc.wasmClientID, cosmosSequence)
			s.Require().Error(err, "pending transfer should be cleared after ack")
		}))
	}))

	ethSequence := uint64(1)
	var ethSendTxHash []byte
	var cosmosRecvTxResponse *sdk.TxResponse
	s.Require().True(s.Run("Transfer Ethereum to Cosmos", func() {
		s.Require().True(s.Run("Execute IFT transfer from Ethereum", func() {
			iftContract, err := evmift.NewContract(tc.ethIFTAddress, eth.RPCClient)
			s.Require().NoError(err)

			txOpts, err := eth.GetTransactOpts(s.ethUser)
			s.Require().NoError(err)

			timeout := uint64(time.Now().Add(30 * time.Minute).Unix())
			tx, err := iftContract.IftTransfer(txOpts, tc.tmClientID, s.CosmosUser.FormattedAddress(), transferAmount.BigInt(), timeout)
			s.Require().NoError(err)

			receipt, err := eth.GetTxReciept(ctx, tx.Hash())
			s.Require().NoError(err)
			s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)
			ethSendTxHash = receipt.TxHash.Bytes()
		}))

		s.Require().True(s.Run("Verify tokens burned on Ethereum", func() {
			iftContract, err := evmift.NewContract(tc.ethIFTAddress, eth.RPCClient)
			s.Require().NoError(err)

			balance, err := iftContract.BalanceOf(nil, ethReceiverAddr)
			s.Require().NoError(err)
			s.Require().True(balance.Cmp(big.NewInt(0)) == 0, "Ethereum sender should have 0 tokens after transfer")
		}))

		s.Require().True(s.Run("Relay packet to Cosmos", func() {
			resp, err := s.RelayerClient.RelayByTx(ctx, &relayertypes.RelayByTxRequest{
				SrcChain:    eth.ChainID.String(),
				DstChain:    s.Wfchain.Config().ChainID,
				SourceTxIds: [][]byte{ethSendTxHash},
				SrcClientId: tc.tmClientID,
				DstClientId: tc.wasmClientID,
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Tx)

			cosmosRecvTxResponse = s.MustBroadcastSdkTxBody(ctx, s.Wfchain, s.CosmosRelayerSubmitter, 2_000_000, resp.Tx)
		}))

		s.Require().True(s.Run("Verify balance on Cosmos", func() {
			balance := s.queryCosmosBalance(ctx, s.CosmosUser.FormattedAddress(), tc.cosmosDenom)
			s.Require().True(balance.Equal(transferAmount), "Expected %s, got %s", transferAmount.String(), balance.String())
		}))

		s.Require().True(s.Run("Relay ack to Ethereum", func() {
			cosmosRecvTxHashBytes, err := hex.DecodeString(cosmosRecvTxResponse.TxHash)
			s.Require().NoError(err)

			resp, err := s.RelayerClient.RelayByTx(ctx, &relayertypes.RelayByTxRequest{
				SrcChain:    s.Wfchain.Config().ChainID,
				DstChain:    eth.ChainID.String(),
				SourceTxIds: [][]byte{cosmosRecvTxHashBytes},
				SrcClientId: tc.wasmClientID,
				DstClientId: tc.tmClientID,
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Tx)

			receipt, err := eth.BroadcastTx(ctx, s.EthRelayerSubmitter, 15_000_000, &tc.ics26Address, resp.Tx)
			s.Require().NoError(err)
			s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)
		}))

		s.Require().True(s.Run("Verify pending transfer cleared on Ethereum", func() {
			iftContract, err := evmift.NewContract(tc.ethIFTAddress, eth.RPCClient)
			s.Require().NoError(err)

			_, err = iftContract.GetPendingTransfer(nil, tc.tmClientID, ethSequence)
			s.Require().Error(err, "getPendingTransfer should revert when transfer is cleared")
		}))
	}))

	s.Require().True(s.Run("Verify final balances", func() {
		s.Require().True(s.Run("Cosmos user has tokens back", func() {
			balance := s.queryCosmosBalance(ctx, s.CosmosUser.FormattedAddress(), tc.cosmosDenom)
			s.Require().True(balance.Equal(transferAmount), "Cosmos user should have tokens back after roundtrip")
		}))

		s.Require().True(s.Run("Ethereum user has no tokens", func() {
			iftContract, err := evmift.NewContract(tc.ethIFTAddress, eth.RPCClient)
			s.Require().NoError(err)

			balance, err := iftContract.BalanceOf(nil, ethReceiverAddr)
			s.Require().NoError(err)
			s.Require().True(balance.Cmp(big.NewInt(0)) == 0, "Ethereum user should have 0 tokens after roundtrip")
		}))
	}))
}

func (s *CosmosEthereumIFTTestSuite) Test_IFTTransfer_TimeoutCosmosToEthereum() {
	ctx := context.Background()
	s.SetupSuite(ctx, types.ProofTypeGroth16)

	eth := s.Eth.Chains[0]
	transferAmount := sdkmath.NewInt(1_000_000)

	tc := s.setupIFTInfrastructure(ctx)

	ethUserAddr := crypto.PubkeyToAddress(s.ethUser.PublicKey)

	s.Require().True(s.Run("Mint tokens to user on Cosmos", func() {
		s.mintTokensOnCosmos(ctx, s.CosmosRelayerSubmitter, tc.cosmosDenom, transferAmount, s.CosmosUser.FormattedAddress())
		balance := s.queryCosmosBalance(ctx, s.CosmosUser.FormattedAddress(), tc.cosmosDenom)
		s.Require().True(balance.Equal(transferAmount))
	}))

	var sendTxHash string
	s.Require().True(s.Run("Send transfer with short timeout", func() {
		timeout := uint64(time.Now().Add(30 * time.Second).Unix())
		sendTxHash = s.iftTransferFromCosmos(
			ctx,
			s.CosmosUser,
			tc.cosmosDenom,
			tc.wasmClientID,
			ethUserAddr.Hex(),
			transferAmount,
			timeout,
		)
		s.Require().NotEmpty(sendTxHash)
	}))

	s.Require().True(s.Run("Verify balance burned on Cosmos", func() {
		balance := s.queryCosmosBalance(ctx, s.CosmosUser.FormattedAddress(), tc.cosmosDenom)
		s.Require().True(balance.IsZero())
	}))

	s.Require().True(s.Run("Verify pending transfer exists", func() {
		pending, err := s.queryPendingTransferOnCosmos(ctx, tc.cosmosDenom, tc.wasmClientID, 1)
		s.Require().NoError(err)
		s.Require().Equal(s.CosmosUser.FormattedAddress(), pending.Sender)
		s.Require().True(pending.Amount.Equal(transferAmount))
	}))

	s.Require().True(s.Run("Wait for timeout", func() {
		s.T().Log("Waiting 35 seconds for timeout...")
		time.Sleep(35 * time.Second)
	}))

	s.Require().True(s.Run("Relay timeout packet to Cosmos", func() {
		sendTxHashBytes, err := hex.DecodeString(sendTxHash)
		s.Require().NoError(err)

		resp, err := s.RelayerClient.RelayByTx(ctx, &relayertypes.RelayByTxRequest{
			SrcChain:     eth.ChainID.String(),
			DstChain:     s.Wfchain.Config().ChainID,
			TimeoutTxIds: [][]byte{sendTxHashBytes},
			SrcClientId:  tc.tmClientID,
			DstClientId:  tc.wasmClientID,
		})
		s.Require().NoError(err)
		s.Require().NotEmpty(resp.Tx)

		_ = s.MustBroadcastSdkTxBody(ctx, s.Wfchain, s.CosmosRelayerSubmitter, 2_000_000, resp.Tx)
	}))

	s.Require().True(s.Run("Verify tokens refunded on Cosmos", func() {
		balance := s.queryCosmosBalance(ctx, s.CosmosUser.FormattedAddress(), tc.cosmosDenom)
		s.Require().True(balance.Equal(transferAmount), "Expected %s (refunded), got %s", transferAmount.String(), balance.String())
	}))

	s.Require().True(s.Run("Verify pending transfer cleared", func() {
		_, err := s.queryPendingTransferOnCosmos(ctx, tc.cosmosDenom, tc.wasmClientID, 1)
		s.Require().Error(err, "pending transfer should be cleared after timeout")
	}))

	s.Require().True(s.Run("Verify no balance on Ethereum", func() {
		iftContract, err := evmift.NewContract(tc.ethIFTAddress, eth.RPCClient)
		s.Require().NoError(err)

		balance, err := iftContract.BalanceOf(nil, ethUserAddr)
		s.Require().NoError(err)
		s.Require().True(balance.Cmp(big.NewInt(0)) == 0, "Ethereum should have no tokens")
	}))
}

func (s *CosmosEthereumIFTTestSuite) Test_IFTTransfer_TimeoutEthereumToCosmos() {
	ctx := context.Background()
	s.SetupSuite(ctx, types.ProofTypeGroth16)

	eth := s.Eth.Chains[0]
	transferAmount := sdkmath.NewInt(1_000_000)

	tc := s.setupIFTInfrastructure(ctx)

	ethUserAddr := crypto.PubkeyToAddress(s.ethUser.PublicKey)

	s.Require().True(s.Run("Mint tokens on Ethereum", func() {
		iftContract, err := evmift.NewContract(tc.ethIFTAddress, eth.RPCClient)
		s.Require().NoError(err)

		txOpts, err := eth.GetTransactOpts(s.ethDeployer)
		s.Require().NoError(err)

		tx, err := iftContract.Mint(txOpts, ethUserAddr, transferAmount.BigInt())
		s.Require().NoError(err)

		receipt, err := eth.GetTxReciept(ctx, tx.Hash())
		s.Require().NoError(err)
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)

		balance, err := iftContract.BalanceOf(nil, ethUserAddr)
		s.Require().NoError(err)
		s.Require().True(balance.Cmp(transferAmount.BigInt()) == 0)
	}))

	var sendTxHash []byte
	s.Require().True(s.Run("Send transfer with short timeout", func() {
		iftContract, err := evmift.NewContract(tc.ethIFTAddress, eth.RPCClient)
		s.Require().NoError(err)

		txOpts, err := eth.GetTransactOpts(s.ethUser)
		s.Require().NoError(err)

		timeout := uint64(time.Now().Add(30 * time.Second).Unix())
		tx, err := iftContract.IftTransfer(txOpts, tc.tmClientID, s.CosmosUser.FormattedAddress(), transferAmount.BigInt(), timeout)
		s.Require().NoError(err)

		receipt, err := eth.GetTxReciept(ctx, tx.Hash())
		s.Require().NoError(err)
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)
		sendTxHash = receipt.TxHash.Bytes()
	}))

	s.Require().True(s.Run("Verify tokens burned on Ethereum", func() {
		iftContract, err := evmift.NewContract(tc.ethIFTAddress, eth.RPCClient)
		s.Require().NoError(err)

		balance, err := iftContract.BalanceOf(nil, ethUserAddr)
		s.Require().NoError(err)
		s.Require().True(balance.Cmp(big.NewInt(0)) == 0, "Tokens should be burned")
	}))

	s.Require().True(s.Run("Verify pending transfer exists on Ethereum", func() {
		iftContract, err := evmift.NewContract(tc.ethIFTAddress, eth.RPCClient)
		s.Require().NoError(err)

		pending, err := iftContract.GetPendingTransfer(nil, tc.tmClientID, 1)
		s.Require().NoError(err)
		s.Require().Equal(ethUserAddr, pending.Sender)
		s.Require().True(pending.Amount.Cmp(transferAmount.BigInt()) == 0)
	}))

	s.Require().True(s.Run("Wait for timeout", func() {
		s.T().Log("Waiting 35 seconds for timeout...")
		time.Sleep(35 * time.Second)
	}))

	s.Require().True(s.Run("Relay timeout packet to Ethereum", func() {
		resp, err := s.RelayerClient.RelayByTx(ctx, &relayertypes.RelayByTxRequest{
			SrcChain:     s.Wfchain.Config().ChainID,
			DstChain:     eth.ChainID.String(),
			TimeoutTxIds: [][]byte{sendTxHash},
			SrcClientId:  tc.wasmClientID,
			DstClientId:  tc.tmClientID,
		})
		s.Require().NoError(err)
		s.Require().NotEmpty(resp.Tx)

		receipt, err := eth.BroadcastTx(ctx, s.EthRelayerSubmitter, 15_000_000, &tc.ics26Address, resp.Tx)
		s.Require().NoError(err)
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)
	}))

	s.Require().True(s.Run("Verify tokens refunded on Ethereum", func() {
		iftContract, err := evmift.NewContract(tc.ethIFTAddress, eth.RPCClient)
		s.Require().NoError(err)

		balance, err := iftContract.BalanceOf(nil, ethUserAddr)
		s.Require().NoError(err)
		s.Require().True(balance.Cmp(transferAmount.BigInt()) == 0, "Expected %s (refunded), got %s", transferAmount.String(), balance.String())
	}))

	s.Require().True(s.Run("Verify pending transfer cleared on Ethereum", func() {
		iftContract, err := evmift.NewContract(tc.ethIFTAddress, eth.RPCClient)
		s.Require().NoError(err)

		_, err = iftContract.GetPendingTransfer(nil, tc.tmClientID, 1)
		s.Require().Error(err, "getPendingTransfer should revert when transfer is cleared")
	}))

	s.Require().True(s.Run("Verify no balance on Cosmos", func() {
		balance := s.queryCosmosBalance(ctx, s.CosmosUser.FormattedAddress(), tc.cosmosDenom)
		s.Require().True(balance.IsZero(), "Cosmos should have no tokens")
	}))
}

// Test_IFTTransfer_FailedReceiveOnCosmos tests error acknowledgment when Cosmos receive fails.
// The test sends from Ethereum to an invalid Cosmos address. The IFT module on Cosmos fails
// to mint tokens because the receiver address is not a valid bech32 address. This generates
// an error ack that is relayed back to Ethereum, triggering a refund to the sender.
func (s *CosmosEthereumIFTTestSuite) Test_IFTTransfer_FailedReceiveOnCosmos() {
	ctx := context.Background()
	s.SetupSuite(ctx, types.ProofTypeGroth16)

	eth := s.Eth.Chains[0]
	transferAmount := sdkmath.NewInt(1_000_000)

	tc := s.setupIFTInfrastructure(ctx)

	ethUserAddr := crypto.PubkeyToAddress(s.ethUser.PublicKey)
	invalidCosmosAddr := "invalid-cosmos-address"

	s.Require().True(s.Run("Mint tokens on Ethereum", func() {
		iftContract, err := evmift.NewContract(tc.ethIFTAddress, eth.RPCClient)
		s.Require().NoError(err)

		txOpts, err := eth.GetTransactOpts(s.ethDeployer)
		s.Require().NoError(err)

		tx, err := iftContract.Mint(txOpts, ethUserAddr, transferAmount.BigInt())
		s.Require().NoError(err)

		receipt, err := eth.GetTxReciept(ctx, tx.Hash())
		s.Require().NoError(err)
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)

		balance, err := iftContract.BalanceOf(nil, ethUserAddr)
		s.Require().NoError(err)
		s.Require().True(balance.Cmp(transferAmount.BigInt()) == 0)
	}))

	var sendTxHash []byte
	s.Require().True(s.Run("Send transfer to invalid Cosmos address", func() {
		iftContract, err := evmift.NewContract(tc.ethIFTAddress, eth.RPCClient)
		s.Require().NoError(err)

		txOpts, err := eth.GetTransactOpts(s.ethUser)
		s.Require().NoError(err)

		timeout := uint64(time.Now().Add(30 * time.Minute).Unix())
		tx, err := iftContract.IftTransfer(txOpts, tc.tmClientID, invalidCosmosAddr, transferAmount.BigInt(), timeout)
		s.Require().NoError(err)

		receipt, err := eth.GetTxReciept(ctx, tx.Hash())
		s.Require().NoError(err)
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)
		sendTxHash = receipt.TxHash.Bytes()
	}))

	s.Require().True(s.Run("Verify tokens burned on Ethereum", func() {
		iftContract, err := evmift.NewContract(tc.ethIFTAddress, eth.RPCClient)
		s.Require().NoError(err)

		balance, err := iftContract.BalanceOf(nil, ethUserAddr)
		s.Require().NoError(err)
		s.Require().True(balance.Cmp(big.NewInt(0)) == 0, "Tokens should be burned")
	}))

	s.Require().True(s.Run("Verify pending transfer exists on Ethereum", func() {
		iftContract, err := evmift.NewContract(tc.ethIFTAddress, eth.RPCClient)
		s.Require().NoError(err)

		pending, err := iftContract.GetPendingTransfer(nil, tc.tmClientID, 1)
		s.Require().NoError(err)
		s.Require().Equal(ethUserAddr, pending.Sender)
		s.Require().True(pending.Amount.Cmp(transferAmount.BigInt()) == 0)
	}))

	var recvTxHash string
	s.Require().True(s.Run("Relay packet to Cosmos (execution fails)", func() {
		resp, err := s.RelayerClient.RelayByTx(ctx, &relayertypes.RelayByTxRequest{
			SrcChain:    eth.ChainID.String(),
			DstChain:    s.Wfchain.Config().ChainID,
			SourceTxIds: [][]byte{sendTxHash},
			SrcClientId: tc.tmClientID,
			DstClientId: tc.wasmClientID,
		})
		s.Require().NoError(err)
		s.Require().NotEmpty(resp.Tx)

		cosmosRecvTxResponse := s.MustBroadcastSdkTxBody(ctx, s.Wfchain, s.CosmosRelayerSubmitter, 2_000_000, resp.Tx)
		recvTxHash = cosmosRecvTxResponse.TxHash
	}))

	s.Require().True(s.Run("Verify no balance minted on Cosmos", func() {
		balance := s.queryCosmosBalance(ctx, s.CosmosUser.FormattedAddress(), tc.cosmosDenom)
		s.Require().True(balance.IsZero(), "Cosmos user should have no tokens")
	}))

	s.Require().True(s.Run("Relay error ack to Ethereum", func() {
		recvTxHashBytes, err := hex.DecodeString(recvTxHash)
		s.Require().NoError(err)

		resp, err := s.RelayerClient.RelayByTx(ctx, &relayertypes.RelayByTxRequest{
			SrcChain:    s.Wfchain.Config().ChainID,
			DstChain:    eth.ChainID.String(),
			SourceTxIds: [][]byte{recvTxHashBytes},
			SrcClientId: tc.wasmClientID,
			DstClientId: tc.tmClientID,
		})
		s.Require().NoError(err)
		s.Require().NotEmpty(resp.Tx)

		receipt, err := eth.BroadcastTx(ctx, s.EthRelayerSubmitter, 15_000_000, &tc.ics26Address, resp.Tx)
		s.Require().NoError(err)
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)
	}))

	s.Require().True(s.Run("Verify tokens refunded on Ethereum", func() {
		iftContract, err := evmift.NewContract(tc.ethIFTAddress, eth.RPCClient)
		s.Require().NoError(err)

		balance, err := iftContract.BalanceOf(nil, ethUserAddr)
		s.Require().NoError(err)
		s.Require().True(balance.Cmp(transferAmount.BigInt()) == 0, "Expected %s (refunded), got %s", transferAmount.String(), balance.String())
	}))

	s.Require().True(s.Run("Verify pending transfer cleared on Ethereum", func() {
		iftContract, err := evmift.NewContract(tc.ethIFTAddress, eth.RPCClient)
		s.Require().NoError(err)

		_, err = iftContract.GetPendingTransfer(nil, tc.tmClientID, 1)
		s.Require().Error(err, "getPendingTransfer should revert when transfer is cleared")
	}))
}

// Test_IFTTransfer_FailedReceiveOnEthereum tests error acknowledgment when Ethereum receive fails.
// The test intentionally skips registering the IFT bridge on Ethereum while registering it on Cosmos.
// When Cosmos sends an IFT transfer, Ethereum's IFT contract fails because no bridge is registered
// for the client ID. The ICS26 router catches this error and generates an error ack, which is
// relayed back to Cosmos to refund the sender.
func (s *CosmosEthereumIFTTestSuite) Test_IFTTransfer_FailedReceiveOnEthereum() {
	ctx := context.Background()
	s.SetupSuite(ctx, types.ProofTypeGroth16)

	eth := s.Eth.Chains[0]
	transferAmount := sdkmath.NewInt(1_000_000)

	tmClientID := testvalues.CustomClientID
	wasmClientID := testvalues.FirstWasmClientID
	ics26Address := ethcommon.HexToAddress(s.contractAddresses.Ics26Router)
	ethIFTAddress := ethcommon.HexToAddress(s.contractAddresses.Ift)

	var sp1Ics07Address ethcommon.Address

	s.Require().True(s.Run("Setup light clients", func() {
		s.Require().True(s.Run("Create Tendermint light client on Ethereum", func() {
			resp, err := s.RelayerClient.CreateClient(ctx, &relayertypes.CreateClientRequest{
				SrcChain: s.Wfchain.Config().ChainID,
				DstChain: eth.ChainID.String(),
				Parameters: map[string]string{
					testvalues.ParameterKey_Sp1Verifier: s.contractAddresses.VerifierMock,
					testvalues.ParameterKey_ZkAlgorithm: types.ProofTypeGroth16.String(),
				},
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Tx)

			receipt, err := eth.BroadcastTx(ctx, s.EthRelayerSubmitter, 15_000_000, nil, resp.Tx)
			s.Require().NoError(err)
			s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)
			sp1Ics07Address = receipt.ContractAddress
		}))

		s.Require().True(s.Run("Create Ethereum light client on Cosmos", func() {
			checksumHex := s.StoreLightClient(ctx, s.Wfchain, s.CosmosRelayerSubmitter)
			s.Require().NotEmpty(checksumHex)

			resp, err := s.RelayerClient.CreateClient(ctx, &relayertypes.CreateClientRequest{
				SrcChain: eth.ChainID.String(),
				DstChain: s.Wfchain.Config().ChainID,
				Parameters: map[string]string{
					testvalues.ParameterKey_ChecksumHex: checksumHex,
				},
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Tx)

			_ = s.MustBroadcastSdkTxBody(ctx, s.Wfchain, s.CosmosRelayerSubmitter, 20_000_000, resp.Tx)
		}))

		s.Require().True(s.Run("Add client and counterparty on Ethereum", func() {
			ics26Contract, err := ics26router.NewContract(ics26Address, eth.RPCClient)
			s.Require().NoError(err)

			counterpartyInfo := ics26router.IICS02ClientMsgsCounterpartyInfo{
				ClientId:     wasmClientID,
				MerklePrefix: [][]byte{[]byte(ibcexported.StoreKey), []byte("")},
			}

			txOpts, err := eth.GetTransactOpts(s.ethDeployer)
			s.Require().NoError(err)

			tx, err := ics26Contract.AddClient(txOpts, tmClientID, counterpartyInfo, sp1Ics07Address)
			s.Require().NoError(err)

			receipt, err := eth.GetTxReciept(ctx, tx.Hash())
			s.Require().NoError(err)
			s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)
		}))

		s.Require().True(s.Run("Register counterparty on Cosmos", func() {
			merklePathPrefix := [][]byte{[]byte("")}

			_, err := s.BroadcastMessages(ctx, s.Wfchain, s.CosmosRelayerSubmitter, 200_000, &clienttypesv2.MsgRegisterCounterparty{
				ClientId:                 wasmClientID,
				CounterpartyMerklePrefix: merklePathPrefix,
				CounterpartyClientId:     tmClientID,
				Signer:                   s.CosmosRelayerSubmitter.FormattedAddress(),
			})
			s.Require().NoError(err)
		}))
	}))

	var cosmosDenom string
	s.Require().True(s.Run("Setup IFT bridge on Cosmos only", func() {
		s.Require().True(s.Run("Create denom on Cosmos", func() {
			cosmosDenom = s.createTokenFactoryDenom(ctx, s.CosmosRelayerSubmitter, cosmosIFTDenom)
		}))

		s.Require().True(s.Run("Register IFT bridge on Cosmos", func() {
			s.registerIFTBridgeOnCosmos(
				ctx,
				s.CosmosRelayerSubmitter,
				cosmosDenom,
				wasmClientID,
				ethIFTAddress.Hex(),
				iftSendCallConstructorEVM,
			)
		}))
		// NOTE: Intentionally NOT registering the IFT bridge on Ethereum
	}))

	ethReceiverAddr := crypto.PubkeyToAddress(s.ethUser.PublicKey)

	s.Require().True(s.Run("Mint tokens to user on Cosmos", func() {
		s.mintTokensOnCosmos(ctx, s.CosmosRelayerSubmitter, cosmosDenom, transferAmount, s.CosmosUser.FormattedAddress())
		balance := s.queryCosmosBalance(ctx, s.CosmosUser.FormattedAddress(), cosmosDenom)
		s.Require().True(balance.Equal(transferAmount))
	}))

	var sendTxHash string
	s.Require().True(s.Run("Send transfer from Cosmos to Ethereum", func() {
		timeout := uint64(time.Now().Add(30 * time.Minute).Unix())
		sendTxHash = s.iftTransferFromCosmos(
			ctx,
			s.CosmosUser,
			cosmosDenom,
			wasmClientID,
			ethReceiverAddr.Hex(),
			transferAmount,
			timeout,
		)
		s.Require().NotEmpty(sendTxHash)
	}))

	s.Require().True(s.Run("Verify balance burned on Cosmos", func() {
		balance := s.queryCosmosBalance(ctx, s.CosmosUser.FormattedAddress(), cosmosDenom)
		s.Require().True(balance.IsZero())
	}))

	s.Require().True(s.Run("Verify pending transfer exists on Cosmos", func() {
		pending, err := s.queryPendingTransferOnCosmos(ctx, cosmosDenom, wasmClientID, 1)
		s.Require().NoError(err)
		s.Require().Equal(s.CosmosUser.FormattedAddress(), pending.Sender)
		s.Require().True(pending.Amount.Equal(transferAmount))
	}))

	var recvTxHash []byte
	s.Require().True(s.Run("Relay packet to Ethereum (execution fails - no bridge registered)", func() {
		sendTxHashBytes, err := hex.DecodeString(sendTxHash)
		s.Require().NoError(err)

		resp, err := s.RelayerClient.RelayByTx(ctx, &relayertypes.RelayByTxRequest{
			SrcChain:    s.Wfchain.Config().ChainID,
			DstChain:    eth.ChainID.String(),
			SourceTxIds: [][]byte{sendTxHashBytes},
			SrcClientId: wasmClientID,
			DstClientId: tmClientID,
		})
		s.Require().NoError(err)
		s.Require().NotEmpty(resp.Tx)

		receipt, err := eth.BroadcastTx(ctx, s.EthRelayerSubmitter, 15_000_000, &ics26Address, resp.Tx)
		s.Require().NoError(err)
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)
		recvTxHash = receipt.TxHash.Bytes()
	}))

	s.Require().True(s.Run("Verify no balance minted on Ethereum", func() {
		iftContract, err := evmift.NewContract(ethIFTAddress, eth.RPCClient)
		s.Require().NoError(err)

		balance, err := iftContract.BalanceOf(nil, ethReceiverAddr)
		s.Require().NoError(err)
		s.Require().True(balance.Cmp(big.NewInt(0)) == 0, "Ethereum should have no tokens")
	}))

	s.Require().True(s.Run("Relay error ack to Cosmos", func() {
		resp, err := s.RelayerClient.RelayByTx(ctx, &relayertypes.RelayByTxRequest{
			SrcChain:    eth.ChainID.String(),
			DstChain:    s.Wfchain.Config().ChainID,
			SourceTxIds: [][]byte{recvTxHash},
			SrcClientId: tmClientID,
			DstClientId: wasmClientID,
		})
		s.Require().NoError(err)
		s.Require().NotEmpty(resp.Tx)

		_ = s.MustBroadcastSdkTxBody(ctx, s.Wfchain, s.CosmosRelayerSubmitter, 2_000_000, resp.Tx)
	}))

	s.Require().True(s.Run("Verify tokens refunded on Cosmos", func() {
		balance := s.queryCosmosBalance(ctx, s.CosmosUser.FormattedAddress(), cosmosDenom)
		s.Require().True(balance.Equal(transferAmount), "Expected %s (refunded), got %s", transferAmount.String(), balance.String())
	}))

	s.Require().True(s.Run("Verify pending transfer cleared on Cosmos", func() {
		_, err := s.queryPendingTransferOnCosmos(ctx, cosmosDenom, wasmClientID, 1)
		s.Require().Error(err, "pending transfer should be cleared after error ack")
	}))
}
