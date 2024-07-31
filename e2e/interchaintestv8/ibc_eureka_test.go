package main

import (
	"context"
	"crypto/ecdsa"
	"encoding/hex"
	"os"
	"strconv"
	"testing"

	"github.com/stretchr/testify/suite"

	ethcommon "github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/crypto"
	"github.com/ethereum/go-ethereum/ethclient"

	"github.com/strangelove-ventures/interchaintest/v8/chain/ethereum"
	"github.com/strangelove-ventures/interchaintest/v8/ibc"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/e2esuite"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/operator"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
)

// IbcEurekaTestSuite is a suite of tests that wraps TestSuite
// and can provide additional functionality
type IbcEurekaTestSuite struct {
	e2esuite.TestSuite

	// The private key of a test account
	key *ecdsa.PrivateKey

	// The latest height of client state
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
		))

		stdout, _, err := eth.ForgeScript(ctx, s.UserA.KeyName(), ethereum.ForgeScriptOpts{
			ContractRootDir:  ".",
			SolidityContract: "contracts/script/IbcEureka.s.sol",
			RawOptions:       []string{"--json"},
		})
		s.Require().NoError(err)

		contractAddress := s.GetEthAddressFromStdout(string(stdout))
		s.Require().NotEmpty(contractAddress)
		s.Require().True(ethcommon.IsHexAddress(contractAddress))

		os.Setenv(testvalues.EnvKeyContractAddress, contractAddress)

		_, err = ethclient.Dial(eth.GetHostRPCAddress())
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
	}))
}
