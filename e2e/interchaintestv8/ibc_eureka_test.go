package main

import (
	"context"
	"crypto/ecdsa"
	"encoding/hex"
	"fmt"
	"os"
	"strconv"
	"testing"
	"time"

	"github.com/stretchr/testify/suite"

	ethcommon "github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/crypto"
	"github.com/ethereum/go-ethereum/ethclient"

	clienttypes "github.com/cosmos/ibc-go/v8/modules/core/02-client/types"
	commitmenttypes "github.com/cosmos/ibc-go/v8/modules/core/23-commitment/types"
	ibcexported "github.com/cosmos/ibc-go/v8/modules/core/exported"
	mock "github.com/cosmos/ibc-go/v8/modules/light-clients/00-mock"
	ibctesting "github.com/cosmos/ibc-go/v8/testing"

	"github.com/strangelove-ventures/interchaintest/v8/chain/ethereum"
	"github.com/strangelove-ventures/interchaintest/v8/ibc"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/e2esuite"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/operator"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/erc20"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/ics02client"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/ics20transfer"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/ics26router"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/sp1ics07tendermint"
)

// IbcEurekaTestSuite is a suite of tests that wraps TestSuite
// and can provide additional functionality
type IbcEurekaTestSuite struct {
	e2esuite.TestSuite

	// The private key of a test account
	key *ecdsa.PrivateKey

	deployedContractAddresses e2esuite.DeployedContracts
	sp1Ics07Contract          *sp1ics07tendermint.Contract
	ics02Contract             *ics02client.Contract
	ics26Contract             *ics26router.Contract
	ics20Contract             *ics20transfer.Contract
	erc20Contract             *erc20.Contract

	simdClientID string
	ethClientID  string

	// The latest height of sp1 ics07 client state
	// nolint: unused
	latestHeight uint32
}

// SetupSuite calls the underlying IbcEurekaTestSuite's SetupSuite method
// and deploys the IbcEureka contract
func (s *IbcEurekaTestSuite) SetupSuite(ctx context.Context) {
	s.TestSuite.SetupSuite(ctx)

	eth, simd := s.ChainA, s.ChainB

	s.Require().True(s.Run("Set up environment", func() {
		err := os.Chdir("../..")
		s.Require().NoError(err)

		s.key, err = crypto.GenerateKey()
		s.Require().NoError(err)
		hexPrivateKey := hex.EncodeToString(crypto.FromECDSA(s.key))
		address := crypto.PubkeyToAddress(s.key.PublicKey).Hex()
		s.T().Logf("Generated key: %s", address)

		os.Setenv(testvalues.EnvKeyEthRPC, eth.GetHostRPCAddress())
		os.Setenv(testvalues.EnvKeyTendermintRPC, simd.GetHostRPCAddress())
		os.Setenv(testvalues.EnvKeySp1Prover, "network")
		os.Setenv(testvalues.EnvKeyPrivateKey, hexPrivateKey)
		// make sure that the SP1_PRIVATE_KEY is set.
		s.Require().NotEmpty(os.Getenv(testvalues.EnvKeySp1PrivateKey))

		s.Require().NoError(eth.SendFunds(ctx, "faucet", ibc.WalletAmount{
			Amount:  testvalues.StartingEthBalance,
			Address: address,
		}))
	}))

	s.Require().True(s.Run("Deploy contracts", func() {
		s.Require().NoError(operator.RunGenesis(
			"--trust-level", testvalues.DefaultTrustLevel.String(),
			"--trusting-period", strconv.Itoa(testvalues.DefaultTrustPeriod),
			"-o", "e2e/artifacts/genesis.json",
		))

		stdout, stderr, err := eth.ForgeScript(ctx, s.UserA.KeyName(), ethereum.ForgeScriptOpts{
			ContractRootDir:  ".",
			SolidityContract: "script/E2ETestDeploy.s.sol",
			RawOptions:       []string{"--json"},
		})
		s.Require().NoError(err, fmt.Sprintf("error deploying contracts: \nstderr: %s\nstdout: %s", stderr, stdout))

		client, err := ethclient.Dial(eth.GetHostRPCAddress())
		s.Require().NoError(err)

		s.deployedContractAddresses = s.GetEthContractsFromDeployOutput(string(stdout))
		s.sp1Ics07Contract, err = sp1ics07tendermint.NewContract(ethcommon.HexToAddress(s.deployedContractAddresses.Ics07Tendermint), client)
		s.Require().NoError(err)
		s.ics02Contract, err = ics02client.NewContract(ethcommon.HexToAddress(s.deployedContractAddresses.Ics02Client), client)
		s.Require().NoError(err)
		s.ics26Contract, err = ics26router.NewContract(ethcommon.HexToAddress(s.deployedContractAddresses.Ics26Router), client)
		s.Require().NoError(err)
		s.ics20Contract, err = ics20transfer.NewContract(ethcommon.HexToAddress(s.deployedContractAddresses.Ics20Transfer), client)
		s.Require().NoError(err)
		s.erc20Contract, err = erc20.NewContract(ethcommon.HexToAddress(s.deployedContractAddresses.Erc20), client)
		s.Require().NoError(err)

		_, err = ethclient.Dial(eth.GetHostRPCAddress())
		s.Require().NoError(err)
	}))

	_, simdRelayerUser := s.GetRelayerUsers(ctx)
	s.Require().True(s.Run("Add client on Cosmos side", func() {
		ethHeight, err := eth.Height(ctx)
		s.Require().NoError(err)

		clientState := mock.ClientState{
			LatestHeight: clienttypes.NewHeight(1, uint64(ethHeight)),
		}
		clientStateAny, err := clienttypes.PackClientState(&clientState)
		s.Require().NoError(err)
		consensusState := mock.ConsensusState{
			Timestamp: uint64(time.Now().UnixNano()),
		}
		consensusStateAny, err := clienttypes.PackConsensusState(&consensusState)
		s.Require().NoError(err)

		res, err := s.BroadcastMessages(ctx, simd, simdRelayerUser, 200_000, &clienttypes.MsgCreateClient{
			ClientState:      clientStateAny,
			ConsensusState:   consensusStateAny,
			Signer:           simdRelayerUser.FormattedAddress(),
			CounterpartyId:   "",
			MerklePathPrefix: nil,
		})
		s.Require().NoError(err)

		s.simdClientID, err = ibctesting.ParseClientIDFromEvents(res.Events)
		s.Require().NoError(err)
		s.Require().Equal("00-mock-0", s.simdClientID)
	}))

	s.Require().True(s.Run("Add client on EVM", func() {
		counterpartyInfo := ics02client.IICS02ClientMsgsCounterpartyInfo{
			ClientId: s.simdClientID,
		}
		lightClientAddress := ethcommon.HexToAddress(s.deployedContractAddresses.Ics07Tendermint)
		tx, err := s.ics02Contract.AddClient(s.GetTransactOpts(s.key), ibcexported.Tendermint, counterpartyInfo, lightClientAddress)
		s.Require().NoError(err)

		receipt := s.GetTxReciept(ctx, eth, tx.Hash())
		event, err := e2esuite.GetEvmEvent[ics02client.ContractICS02ClientAdded](receipt, s.ics02Contract.ParseICS02ClientAdded)
		s.Require().NoError(err)
		s.Require().Equal(ibctesting.FirstClientID, event.ClientId)
		s.Require().Equal(s.simdClientID, event.CounterpartyInfo.ClientId)
		s.ethClientID = event.ClientId
	}))

	s.Require().True(s.Run("Register counterparty on Cosmos side", func() {
		// NOTE: This is the mock client on the Cosmos side, so the prefix need not be valid
		merklePathPrefix := commitmenttypes.NewMerklePath([]byte{0x1})

		_, err := s.BroadcastMessages(ctx, simd, simdRelayerUser, 200_000, &clienttypes.MsgProvideCounterparty{
			ClientId:         s.simdClientID,
			CounterpartyId:   s.ethClientID,
			MerklePathPrefix: &merklePathPrefix,
			Signer:           simdRelayerUser.FormattedAddress(),
		})
		s.Require().NoError(err)
	}))
}

// TestWithIbcEurekaTestSuite is the boilerplate code that allows the test suite to be run
func TestWithIbcEurekaTestSuite(t *testing.T) {
	suite.Run(t, new(IbcEurekaTestSuite))
}

// TestDeploy tests the deployment of the IbcEureka contracts
func (s *IbcEurekaTestSuite) TestDeploy() {
	ctx := context.Background()

	s.SetupSuite(ctx)

	_, _ = s.ChainA, s.ChainB

	s.Require().True(s.Run("Verify deployment", func() {
		// Verify that the contracts have been deployed
		s.Require().NotNil(s.sp1Ics07Contract)
		s.Require().NotNil(s.ics02Contract)
		s.Require().NotNil(s.ics26Contract)
		s.Require().NotNil(s.ics20Contract)
		s.Require().NotNil(s.erc20Contract)
	}))
}

func (s *IbcEurekaTestSuite) TestICS20Transfer() {
	ctx := context.Background()

	s.SetupSuite(ctx)

	// eth, simd := s.ChainA, s.ChainB
}
