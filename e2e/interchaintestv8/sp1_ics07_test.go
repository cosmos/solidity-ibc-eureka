package main

import (
	"context"
	"crypto/ecdsa"
	"crypto/rand"
	"encoding/hex"
	"fmt"
	mathrand "math/rand"
	"os"
	"strconv"
	"testing"
	"time"

	"github.com/stretchr/testify/suite"

	ethcommon "github.com/ethereum/go-ethereum/common"
	ethtypes "github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/crypto"

	"cosmossdk.io/math"

	"github.com/cosmos/cosmos-sdk/crypto/keys/ed25519"
	sdk "github.com/cosmos/cosmos-sdk/types"
	banktypes "github.com/cosmos/cosmos-sdk/x/bank/types"

	abci "github.com/cometbft/cometbft/abci/types"

	ibcclientutils "github.com/cosmos/ibc-go/v10/modules/core/02-client/client/utils"
	clienttypes "github.com/cosmos/ibc-go/v10/modules/core/02-client/types"
	ibchostv2 "github.com/cosmos/ibc-go/v10/modules/core/24-host/v2"
	ibcexported "github.com/cosmos/ibc-go/v10/modules/core/exported"
	tmclient "github.com/cosmos/ibc-go/v10/modules/light-clients/07-tendermint"
	ibctesting "github.com/cosmos/ibc-go/v10/testing"

	"github.com/cosmos/solidity-ibc-eureka/packages/go-abigen/ics26router"
	"github.com/cosmos/solidity-ibc-eureka/packages/go-abigen/sp1ics07tendermint"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/cosmos"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/e2esuite"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/ethereum"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/operator"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/relayer"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types"
	relayertypes "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/relayer"
)

// SP1ICS07TendermintTestSuite is a suite of tests that wraps TestSuite
// and can provide additional functionality
type SP1ICS07TendermintTestSuite struct {
	e2esuite.TestSuite

	// Whether to generate fixtures for the solidity tests
	generateFixtures bool

	// Addresses of the deployed contracts
	sp1Ics07Address ethcommon.Address
	ics26Address    ethcommon.Address

	// The private key of a test account
	key *ecdsa.PrivateKey
	// The SP1ICS07Tendermint contract
	contract *sp1ics07tendermint.Contract
	// The ICS26 router contract, needed for the relayer to pass proofs
	ics26Contract *ics26router.Contract

	// The relayer API (only used for deployment at the moment)
	RelayerClient relayertypes.RelayerServiceClient
}

// SetupSuite calls the underlying SP1ICS07TendermintTestSuite's SetupSuite method
// and deploys the SP1ICS07Tendermint contract
func (s *SP1ICS07TendermintTestSuite) SetupSuite(ctx context.Context, proofType types.SupportedProofType) {
	s.TestSuite.SetupSuite(ctx)

	eth, simd := s.EthChain, s.CosmosChains[0]

	s.T().Logf("Setting up the test suite with proof type: %s", proofType.String())

	var prover string
	s.Require().True(s.Run("Set up environment", func() {
		err := os.Chdir("../..")
		s.Require().NoError(err)

		s.key, err = eth.CreateAndFundUser()
		s.Require().NoError(err)

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

		os.Setenv(testvalues.EnvKeyRustLog, testvalues.EnvValueRustLog_Info)
		os.Setenv(testvalues.EnvKeyEthRPC, eth.RPC)
		os.Setenv(testvalues.EnvKeyTendermintRPC, simd.GetHostRPCAddress())
		os.Setenv(testvalues.EnvKeyOperatorPrivateKey, hex.EncodeToString(crypto.FromECDSA(s.key)))
		s.generateFixtures = os.Getenv(testvalues.EnvKeyGenerateSolidityFixtures) == testvalues.EnvValueGenerateFixtures_True
	}))

	s.Require().True(s.Run("Deploy IBC contracts", func() {
		stdout, err := eth.ForgeScript(s.key, testvalues.E2EDeployScriptPath)
		s.Require().NoError(err)

		contractAddresses, err := ethereum.GetEthContractsFromDeployOutput(string(stdout))
		s.Require().NoError(err)
		s.ics26Address = ethcommon.HexToAddress(contractAddresses.Ics26Router)
		s.ics26Contract, err = ics26router.NewContract(s.ics26Address, eth.RPCClient)
		s.Require().NoError(err)
	}))

	var relayerProcess *os.Process
	s.Require().True(s.Run("Start Relayer", func() {
		beaconAPI := ""
		// The BeaconAPIClient is nil when the testnet is `pow`
		if eth.BeaconAPIClient != nil {
			beaconAPI = eth.BeaconAPIClient.GetBeaconAPIURL()
		}

		sp1Config := relayer.SP1ProverConfig{
			Type:           prover,
			PrivateCluster: os.Getenv(testvalues.EnvKeyNetworkPrivateCluster) == testvalues.EnvValueSp1Prover_PrivateCluster,
		}

		config := relayer.NewConfig(relayer.CreateEthCosmosModules(
			relayer.EthCosmosConfigInfo{
				EthChainID:     eth.ChainID.String(),
				CosmosChainID:  simd.Config().ChainID,
				TmRPC:          simd.GetHostRPCAddress(),
				ICS26Address:   s.ics26Address.Hex(),
				EthRPC:         eth.RPC,
				BeaconAPI:      beaconAPI,
				SP1Config:      sp1Config,
				SignerAddress:  "",   // unused
				MockWasmClient: true, // unused
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

	s.Require().True(s.Run("Deploy SP1 ICS07 contract", func() {
		stdout, err := eth.ForgeScript(s.key, testvalues.E2EDeployScriptPath)
		s.Require().NoError(err)

		contractAddresses, err := ethereum.GetEthContractsFromDeployOutput(string(stdout))
		s.Require().NoError(err)

		var verfierAddress string
		if prover == testvalues.EnvValueSp1Prover_Mock {
			verfierAddress = contractAddresses.VerifierMock
		} else {
			switch proofType {
			case types.ProofTypeGroth16:
				verfierAddress = contractAddresses.VerifierGroth16
			case types.ProofTypePlonk:
				verfierAddress = contractAddresses.VerifierPlonk
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
					testvalues.ParameterKey_RoleManager: ethcommon.Address{}.Hex(),
				},
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Tx)
			s.Require().Empty(resp.Address)

			createClientTxBz = resp.Tx
		}))

		s.Require().True(s.Run("Broadcast relay tx", func() {
			receipt, err := eth.BroadcastTx(ctx, s.key, 15_000_000, nil, createClientTxBz)
			s.Require().NoError(err)
			s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status, fmt.Sprintf("Tx failed: %+v", receipt))
			s.Require().NotEmpty(receipt.ContractAddress.Hex())
			s.sp1Ics07Address = receipt.ContractAddress

			s.contract, err = sp1ics07tendermint.NewContract(receipt.ContractAddress, eth.RPCClient)
			s.Require().NoError(err)
		}))

		s.Require().True(s.Run("Add client and counterparty on EVM", func() {
			counterpartyInfo := ics26router.IICS02ClientMsgsCounterpartyInfo{
				ClientId:     testvalues.FirstWasmClientID,
				MerklePrefix: [][]byte{[]byte(ibcexported.StoreKey), []byte("")},
			}
			tx, err := s.ics26Contract.AddClient(s.GetTransactOpts(s.key, eth), testvalues.CustomClientID, counterpartyInfo, s.sp1Ics07Address)
			s.Require().NoError(err)

			receipt, err := eth.GetTxReciept(ctx, tx.Hash())
			s.Require().NoError(err)

			event, err := e2esuite.GetEvmEvent(receipt, s.ics26Contract.ParseICS02ClientAdded)
			s.Require().NoError(err)
			s.Require().Equal(testvalues.CustomClientID, event.ClientId)
			s.Require().Equal(testvalues.FirstWasmClientID, event.CounterpartyInfo.ClientId)
		}))
	}))
}

// TestWithSP1ICS07TendermintTestSuite is the boilerplate code that allows the test suite to be run
func TestWithSP1ICS07TendermintTestSuite(t *testing.T) {
	suite.Run(t, new(SP1ICS07TendermintTestSuite))
}

func (s *SP1ICS07TendermintTestSuite) Test_Deploy() {
	ctx := context.Background()
	proofType := types.GetEnvProofType()
	s.DeployTest(ctx, proofType)
}

// DeployTest tests the deployment of the SP1ICS07Tendermint contract with the given arguments
func (s *SP1ICS07TendermintTestSuite) DeployTest(ctx context.Context, proofType types.SupportedProofType) {
	s.SetupSuite(ctx, proofType)

	_, simd := s.EthChain, s.CosmosChains[0]

	s.Require().True(s.Run("Verify deployment", func() {
		clientState, err := s.contract.ClientState(nil)
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
}

func (s *SP1ICS07TendermintTestSuite) Test_UpdateClient() {
	ctx := context.Background()
	proofType := types.GetEnvProofType()
	s.UpdateClientTest(ctx, proofType)
}

// UpdateClientTest tests the update client functionality
func (s *SP1ICS07TendermintTestSuite) UpdateClientTest(ctx context.Context, proofType types.SupportedProofType) {
	s.SetupSuite(ctx, proofType)

	_, simd := s.EthChain, s.CosmosChains[0]

	if s.generateFixtures {
		s.T().Log("Generate fixtures is set to true, but TestUpdateClient does not support it (yet)")
	}

	s.Require().True(s.Run("Update client", func() {
		clientState, err := s.contract.ClientState(nil)
		s.Require().NoError(err)

		initialHeight := clientState.LatestHeight.RevisionHeight

		s.UpdateClient(ctx)

		clientState, err = s.contract.ClientState(nil)
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
		s.Require().Greater(clientState.LatestHeight.RevisionHeight, initialHeight)
	}))
}

func (s *SP1ICS07TendermintTestSuite) Test_Membership() {
	ctx := context.Background()
	proofType := types.GetEnvProofType()
	s.MembershipTest(ctx, proofType)
}

// MembershipTest tests the verify (non)membership functionality with the given arguments
func (s *SP1ICS07TendermintTestSuite) MembershipTest(ctx context.Context, proofType types.SupportedProofType) {
	s.SetupSuite(ctx, proofType)

	eth, simd := s.EthChain, s.CosmosChains[0]
	simdUser := s.CosmosUsers[0]

	if s.generateFixtures {
		s.T().Log("Generate fixtures is set to true, but TestVerifyMembership does not support it (yet)")
	}

	s.Require().True(s.Run("Verify membership", func() {
		var membershipKey [][]byte
		s.Require().True(s.Run("Generate keys", func() {
			// Prove the bank balance of UserA
			key, err := cosmos.BankBalanceKey(simdUser.Address(), simd.Config().Denom)
			s.Require().NoError(err)

			membershipKey = [][]byte{[]byte(banktypes.StoreKey), key}
		}))

		clientState, err := s.contract.ClientState(nil)
		s.Require().NoError(err)

		trustedHeight := clientState.LatestHeight.RevisionHeight

		var expValue []byte
		s.Require().True(s.Run("Get expected value for the verify membership", func() {
			resp, err := e2esuite.ABCIQuery(ctx, simd, &abci.RequestQuery{
				Path:   "store/" + string(membershipKey[0]) + "/key",
				Data:   membershipKey[1],
				Height: int64(trustedHeight) - 1,
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Value)

			expValue = resp.Value
		}))

		memArgs := append([]string{"--trust-level", testvalues.DefaultTrustLevel.String(), "--trusting-period", strconv.Itoa(testvalues.DefaultTrustPeriod), "--base64"}, proofType.ToOperatorArgs()...)
		proofHeight, ucAndMemProof, err := operator.MembershipProof(
			trustedHeight, operator.ToBase64KeyPaths(membershipKey), "",
			memArgs...,
		)
		s.Require().NoError(err)

		msg := sp1ics07tendermint.ILightClientMsgsMsgVerifyMembership{
			ProofHeight: *proofHeight,
			Proof:       ucAndMemProof,
			Path:        membershipKey,
			Value:       expValue,
		}

		tx, err := s.contract.VerifyMembership(s.GetTransactOpts(s.key, eth), msg)
		s.Require().NoError(err)

		// wait until transaction is included in a block
		receipt, err := eth.GetTxReciept(ctx, tx.Hash())
		s.Require().NoError(err)
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status, fmt.Sprintf("Tx failed: %+v", receipt))
		s.T().Logf("Gas used in %s: %d", s.T().Name(), receipt.GasUsed)
	}))

	s.Require().True(s.Run("Verify non-membership", func() {
		var nonMembershipKey [][]byte
		s.Require().True(s.Run("Generate keys", func() {
			// A non-membership key:
			packetReceiptPath := ibchostv2.PacketReceiptKey(ibctesting.FirstChannelID, 1)

			nonMembershipKey = [][]byte{[]byte(ibcexported.StoreKey), packetReceiptPath}
		}))

		clientState, err := s.contract.ClientState(nil)
		s.Require().NoError(err)

		trustedHeight := clientState.LatestHeight.RevisionHeight

		nonMemArgs := append([]string{"--trust-level", testvalues.DefaultTrustLevel.String(), "--trusting-period", strconv.Itoa(testvalues.DefaultTrustPeriod), "--base64"}, proofType.ToOperatorArgs()...)
		proofHeight, ucAndMemProof, err := operator.MembershipProof(
			trustedHeight, operator.ToBase64KeyPaths(nonMembershipKey), "",
			nonMemArgs...,
		)
		s.Require().NoError(err)

		msg := sp1ics07tendermint.ILightClientMsgsMsgVerifyNonMembership{
			ProofHeight: *proofHeight,
			Proof:       ucAndMemProof,
			Path:        nonMembershipKey,
		}

		tx, err := s.contract.VerifyNonMembership(s.GetTransactOpts(s.key, eth), msg)
		s.Require().NoError(err)

		// wait until transaction is included in a block
		receipt, err := eth.GetTxReciept(ctx, tx.Hash())
		s.Require().NoError(err)
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status, fmt.Sprintf("Tx failed: %+v", receipt))
		s.T().Logf("Gas used in %s: %d", s.T().Name(), receipt.GasUsed)
	}))
}

func (s *SP1ICS07TendermintTestSuite) Test_UpdateClientAndMembership() {
	ctx := context.Background()
	proofType := types.GetEnvProofType()
	s.UpdateClientAndMembershipTest(ctx, proofType)
}

// UpdateClientAndMembershipTest tests the update client and membership functionality with the given arguments
func (s *SP1ICS07TendermintTestSuite) UpdateClientAndMembershipTest(ctx context.Context, proofType types.SupportedProofType) {
	s.SetupSuite(ctx, proofType)

	eth, simd := s.EthChain, s.CosmosChains[0]
	simdUser := s.CosmosUsers[0]

	if s.generateFixtures {
		s.T().Log("Generate fixtures is set to true, but TestUpdateClientAndMembership does not support it (yet)")
	}

	s.Require().True(s.Run("Update and verify (non)membership", func() {
		var (
			membershipKey    [][]byte
			nonMembershipKey [][]byte
		)
		s.Require().True(s.Run("Generate keys", func() {
			// Prove the bank balance of UserA
			key, err := cosmos.BankBalanceKey(simdUser.Address(), simd.Config().Denom)
			s.Require().NoError(err)

			membershipKey = [][]byte{[]byte(banktypes.StoreKey), key}

			// A non-membership key:
			packetReceiptPath := ibchostv2.PacketReceiptKey(ibctesting.FirstChannelID, 1)

			nonMembershipKey = [][]byte{[]byte(ibcexported.StoreKey), packetReceiptPath}
		}))

		clientState, err := s.contract.ClientState(nil)
		s.Require().NoError(err)

		trustedHeight := clientState.LatestHeight.RevisionHeight

		latestHeight, err := simd.Height(ctx)
		s.Require().NoError(err)

		s.Require().Greater(uint64(latestHeight), trustedHeight)

		var expValue []byte
		s.Require().True(s.Run("Get expected value for the verify membership", func() {
			resp, err := e2esuite.ABCIQuery(ctx, simd, &abci.RequestQuery{
				Path:   "store/" + string(membershipKey[0]) + "/key",
				Data:   membershipKey[1],
				Height: latestHeight - 1,
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Value)

			expValue = resp.Value
		}))

		args := append([]string{"--trust-level", testvalues.DefaultTrustLevel.String(), "--trusting-period", strconv.Itoa(testvalues.DefaultTrustPeriod), "--base64"}, proofType.ToOperatorArgs()...)
		proofHeight, ucAndMemProof, err := operator.UpdateClientAndMembershipProof(
			trustedHeight, uint64(latestHeight),
			operator.ToBase64KeyPaths(membershipKey, nonMembershipKey),
			args...,
		)
		s.Require().NoError(err)

		msg := sp1ics07tendermint.ILightClientMsgsMsgVerifyMembership{
			ProofHeight: *proofHeight,
			Proof:       ucAndMemProof,
			Path:        membershipKey,
			Value:       expValue,
		}

		tx, err := s.contract.VerifyMembership(s.GetTransactOpts(s.key, eth), msg)
		s.Require().NoError(err)

		// wait until transaction is included in a block
		receipt, err := eth.GetTxReciept(ctx, tx.Hash())
		s.Require().NoError(err)
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status, fmt.Sprintf("Tx failed: %+v", receipt))
		s.T().Logf("Gas used in %s: %d", s.T().Name(), receipt.GasUsed)

		clientState, err = s.contract.ClientState(nil)
		s.Require().NoError(err)

		s.Require().Equal(uint64(1), clientState.LatestHeight.RevisionNumber)
		s.Require().Greater(clientState.LatestHeight.RevisionHeight, trustedHeight)
		s.Require().Equal(proofHeight.RevisionHeight, clientState.LatestHeight.RevisionHeight)
		s.Require().False(clientState.IsFrozen)
	}))
}

func (s *SP1ICS07TendermintTestSuite) Test_DoubleSignMisbehaviour() {
	ctx := context.Background()
	proofType := types.GetEnvProofType()
	s.DoubleSignMisbehaviourTest(ctx, "double_sign", proofType)
}

// DoubleSignMisbehaviourTest tests the misbehaviour functionality with the given arguments
// Fixture is only generated if the environment variable is set
// Partially based on https://github.com/cosmos/relayer/blob/f9aaf3dd0ebfe99fbe98d190a145861d7df93804/interchaintest/misbehaviour_test.go#L38
func (s *SP1ICS07TendermintTestSuite) DoubleSignMisbehaviourTest(ctx context.Context, fixName string, proofType types.SupportedProofType) {
	s.SetupSuite(ctx, proofType)

	eth, simd := s.EthChain, s.CosmosChains[0]
	_ = eth

	var height clienttypes.Height
	var trustedHeader tmclient.Header
	s.Require().True(s.Run("Get trusted header", func() {
		var latestHeight int64
		var err error
		trustedHeader, latestHeight, err = ibcclientutils.QueryTendermintHeader(simd.Validators[0].CliContext())
		s.Require().NoError(err)
		s.Require().NotZero(latestHeight)

		height = clienttypes.NewHeight(clienttypes.ParseChainID(simd.Config().ChainID), uint64(latestHeight))

		clientState, err := s.contract.ClientState(nil)
		s.Require().NoError(err)
		trustedHeight := clienttypes.NewHeight(clientState.LatestHeight.RevisionNumber, clientState.LatestHeight.RevisionHeight)

		trustedHeader.TrustedHeight = trustedHeight
		trustedHeader.TrustedValidators = trustedHeader.ValidatorSet
	}))

	s.Require().True(s.Run("Invalid misbehaviour", func() {
		// Create a new valid header
		newHeader := s.CreateTMClientHeader(
			ctx,
			simd,
			int64(height.RevisionHeight+1),
			trustedHeader.GetTime().Add(time.Minute),
			trustedHeader,
		)

		invalidMisbehaviour := tmclient.Misbehaviour{
			Header1: &newHeader,
			Header2: &trustedHeader,
		}

		// The proof should fail because this is not misbehaviour (valid header for a new block)
		args := append([]string{
			"--trust-level", testvalues.DefaultTrustLevel.String(),
			"--trusting-period", strconv.Itoa(testvalues.DefaultTrustPeriod),
		},
			proofType.ToOperatorArgs()...,
		)
		_, err := operator.MisbehaviourProof(simd.GetCodec(), invalidMisbehaviour, "", args...)
		s.Require().ErrorContains(err, "misbehaviour is not detected")
	}))

	s.Require().True(s.Run("Valid misbehaviour", func() {
		// create a duplicate header (with a different hash)
		newHeader := s.CreateTMClientHeader(
			ctx,
			simd,
			int64(height.RevisionHeight),
			trustedHeader.GetTime().Add(time.Minute),
			trustedHeader,
		)

		misbehaviour := tmclient.Misbehaviour{
			Header1: &newHeader,
			Header2: &trustedHeader,
		}

		var fixtureName string
		if s.generateFixtures {
			fixtureName = fmt.Sprintf("%s-%s", fixName, proofType.String())
		}
		args := append([]string{
			"--trust-level", testvalues.DefaultTrustLevel.String(),
			"--trusting-period", strconv.Itoa(testvalues.DefaultTrustPeriod),
		},
			proofType.ToOperatorArgs()...,
		)
		submitMsg, err := operator.MisbehaviourProof(simd.GetCodec(), misbehaviour, fixtureName, args...)
		s.Require().NoError(err)

		tx, err := s.contract.Misbehaviour(s.GetTransactOpts(s.key, eth), submitMsg)
		s.Require().NoError(err)

		// wait until transaction is included in a block
		receipt, err := eth.GetTxReciept(ctx, tx.Hash())
		s.Require().NoError(err)
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status, fmt.Sprintf("Tx failed: %+v", receipt))
		s.T().Logf("Gas used in %s: %d", s.T().Name(), receipt.GasUsed)

		clientState, err := s.contract.ClientState(nil)
		s.Require().NoError(err)
		s.Require().True(clientState.IsFrozen)
	}))
}

func (s *SP1ICS07TendermintTestSuite) Test_BreakingTimeMonotonicityMisbehaviour() {
	ctx := context.Background()
	proofType := types.GetEnvProofType()
	s.BreakingTimeMonotonicityMisbehaviourTest(ctx, "breaking_time_monotonicity", proofType)
}

// TestBreakingTimeMonotonicityMisbehaviour tests the misbehaviour functionality
// Fixture is only generated if the environment variable is set
// Partially based on https://github.com/cosmos/relayer/blob/f9aaf3dd0ebfe99fbe98d190a145861d7df93804/interchaintest/misbehaviour_test.go#L38
func (s *SP1ICS07TendermintTestSuite) BreakingTimeMonotonicityMisbehaviourTest(ctx context.Context, fixName string, proofType types.SupportedProofType) {
	s.SetupSuite(ctx, proofType)

	eth, simd := s.EthChain, s.CosmosChains[0]

	var height clienttypes.Height
	var trustedHeader tmclient.Header
	s.Require().True(s.Run("Get trusted header", func() {
		var latestHeight int64
		var err error
		trustedHeader, latestHeight, err = ibcclientutils.QueryTendermintHeader(simd.Validators[0].CliContext())
		s.Require().NoError(err)
		s.Require().NotZero(latestHeight)

		height = clienttypes.NewHeight(clienttypes.ParseChainID(simd.Config().ChainID), uint64(latestHeight))

		clientState, err := s.contract.ClientState(nil)
		s.Require().NoError(err)
		trustedHeight := clienttypes.NewHeight(clientState.LatestHeight.RevisionNumber, clientState.LatestHeight.RevisionHeight)

		trustedHeader.TrustedHeight = trustedHeight
		trustedHeader.TrustedValidators = trustedHeader.ValidatorSet
	}))

	s.Require().True(s.Run("Valid misbehaviour", func() {
		// we have a trusted height n from trustedHeader
		// we now create two new headers n+1 and n+2 where both have time later than n
		// but n+2 has time earlier than n+1, which breaks time monotonicity

		// n+1
		header2 := s.CreateTMClientHeader(
			ctx,
			simd,
			int64(height.RevisionHeight+1),
			trustedHeader.GetTime().Add(time.Minute),
			trustedHeader,
		)

		// n+2 (with time earlier than n+1 and still after n)
		header1 := s.CreateTMClientHeader(
			ctx,
			simd,
			int64(height.RevisionHeight+2),
			trustedHeader.GetTime().Add(time.Minute).Add(-30*time.Second),
			trustedHeader,
		)

		misbehaviour := tmclient.Misbehaviour{
			Header1: &header1,
			Header2: &header2,
		}

		var fixtureName string
		if s.generateFixtures {
			fixtureName = fmt.Sprintf("%s-%s", fixName, proofType.String())
		}
		args := append([]string{
			"--trust-level", testvalues.DefaultTrustLevel.String(),
			"--trusting-period", strconv.Itoa(testvalues.DefaultTrustPeriod),
		},
			proofType.ToOperatorArgs()...,
		)
		submitMsg, err := operator.MisbehaviourProof(simd.GetCodec(), misbehaviour, fixtureName, args...)
		s.Require().NoError(err)

		tx, err := s.contract.Misbehaviour(s.GetTransactOpts(s.key, eth), submitMsg)
		s.Require().NoError(err)

		// wait until transaction is included in a block
		receipt, err := eth.GetTxReciept(ctx, tx.Hash())
		s.Require().NoError(err)
		s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status, fmt.Sprintf("Tx failed: %+v", receipt))
		s.T().Logf("Gas used in %s: %d", s.T().Name(), receipt.GasUsed)

		clientState, err := s.contract.ClientState(nil)
		s.Require().NoError(err)
		s.Require().True(clientState.IsFrozen)
	}))
}

func (s *SP1ICS07TendermintTestSuite) Test_100_Membership() {
	ctx := context.Background()
	proofType := types.GetEnvProofType()

	s.largeMembershipTest(ctx, 100, proofType)
}

func (s *SP1ICS07TendermintTestSuite) Test_25_Membership() {
	ctx := context.Background()
	proofType := types.GetEnvProofType()

	s.largeMembershipTest(ctx, 25, proofType)
}

// largeMembershipTest tests membership proofs with a large number of key-value pairs
func (s *SP1ICS07TendermintTestSuite) largeMembershipTest(ctx context.Context, n uint64, proofType types.SupportedProofType) {
	s.SetupSuite(ctx, proofType)

	eth, simd := s.EthChain, s.CosmosChains[0]
	simdUser := s.CosmosUsers[0]

	s.Require().True(s.Run(fmt.Sprintf("Large membership test with %d key-value pairs", n), func() {
		membershipKeys := make([][][]byte, n)
		s.Require().True(s.Run("Generate state and keys", func() {
			// Messages to generate state to be used in the membership proof
			msgs := []sdk.Msg{}
			// Generate a random addresses
			pubBz := make([]byte, ed25519.PubKeySize)
			pub := &ed25519.PubKey{Key: pubBz}
			for i := uint64(0); i < n; i++ {
				_, err := rand.Read(pubBz)
				s.Require().NoError(err)
				acc := sdk.AccAddress(pub.Address())

				// Send some funds to the address
				msgs = append(msgs, banktypes.NewMsgSend(simdUser.Address(), acc, sdk.NewCoins(sdk.NewCoin(simd.Config().Denom, math.NewInt(1)))))

				key, err := cosmos.BankBalanceKey(simdUser.Address(), simd.Config().Denom)
				s.Require().NoError(err)

				membershipKeys[i] = [][]byte{[]byte(banktypes.StoreKey), key}
			}

			// Send the messages
			_, err := s.BroadcastMessages(ctx, simd, simdUser, 20_000_000, msgs...)
			s.Require().NoError(err)
		}))

		// update the client
		clientHeight := s.UpdateClient(ctx)

		s.Require().True(s.Run("Verify membership", func() {
			rndIdx := mathrand.Intn(int(n))

			var expValue []byte
			s.Require().True(s.Run("Get expected value for the verify membership", func() {
				resp, err := e2esuite.ABCIQuery(ctx, simd, &abci.RequestQuery{
					Path:   fmt.Sprintf("store/%s/key", membershipKeys[rndIdx][0]),
					Data:   membershipKeys[rndIdx][1],
					Height: int64(clientHeight.RevisionHeight) - 1,
				})
				s.Require().NoError(err)
				s.Require().NotEmpty(resp.Value)

				expValue = resp.Value
			}))

			var fixtureName string
			if s.generateFixtures {
				fixtureName = fmt.Sprintf("membership_%d-%s", n, proofType.String())
			}
			args := append([]string{"--trust-level", testvalues.DefaultTrustLevel.String(), "--trusting-period", strconv.Itoa(testvalues.DefaultTrustPeriod), "--base64"}, proofType.ToOperatorArgs()...)
			proofHeight, memProof, err := operator.MembershipProof(
				clientHeight.RevisionHeight, operator.ToBase64KeyPaths(membershipKeys...),
				fixtureName, args...,
			)
			s.Require().NoError(err)

			msg := sp1ics07tendermint.ILightClientMsgsMsgVerifyMembership{
				ProofHeight: *proofHeight,
				Proof:       memProof,
				Path:        membershipKeys[rndIdx],
				Value:       expValue,
			}

			tx, err := s.contract.VerifyMembership(s.GetTransactOpts(s.key, eth), msg)
			s.Require().NoError(err)

			// wait until transaction is included in a block
			receipt, err := eth.GetTxReciept(ctx, tx.Hash())
			s.Require().NoError(err)
			s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status, fmt.Sprintf("Tx failed: %+v", receipt))
		}))
	}))
}

// UpdateClient updates the SP1ICS07Tendermint client and returns the new height
func (s *SP1ICS07TendermintTestSuite) UpdateClient(ctx context.Context) clienttypes.Height {
	eth, simd := s.EthChain, s.CosmosChains[0]

	var initialHeight uint64
	s.Require().True(s.Run("Get the initial height", func() {
		clientState, err := s.contract.ClientState(nil)
		s.Require().NoError(err)
		s.Require().NotZero(clientState.LatestHeight.RevisionHeight)

		initialHeight = clientState.LatestHeight.RevisionHeight
	}))

	var finalHeight sp1ics07tendermint.IICS02ClientMsgsHeight
	s.Require().True(s.Run("Update the client on Ethereum", func() {
		var updateTxBodyBz []byte
		s.Require().True(s.Run("Retrieve relay tx", func() {
			resp, err := s.RelayerClient.UpdateClient(context.Background(), &relayertypes.UpdateClientRequest{
				SrcChain:    simd.Config().ChainID,
				DstChain:    eth.ChainID.String(),
				DstClientId: testvalues.CustomClientID,
			})
			s.Require().NoError(err)
			s.Require().NotEmpty(resp.Tx)
			s.Require().Equal(s.ics26Address.String(), resp.Address)

			updateTxBodyBz = resp.Tx
		}))

		s.Require().True(s.Run("Broadcast relay tx", func() {
			receipt, err := eth.BroadcastTx(ctx, s.key, 5_000_000, &s.ics26Address, updateTxBodyBz)
			s.Require().NoError(err)
			s.Require().Equal(ethtypes.ReceiptStatusSuccessful, receipt.Status)
		}))

		s.Require().True(s.Run("Verify the client state is updated", func() {
			clientState, err := s.contract.ClientState(nil)
			s.Require().NoError(err)
			s.Require().NotZero(clientState.LatestHeight.RevisionHeight)

			finalHeight = clientState.LatestHeight
			s.Require().Greater(finalHeight.RevisionHeight, initialHeight)
		}))
	}))

	return clienttypes.NewHeight(finalHeight.RevisionNumber, finalHeight.RevisionHeight)
}
