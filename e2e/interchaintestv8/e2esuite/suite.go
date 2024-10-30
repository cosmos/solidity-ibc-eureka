package e2esuite

import (
	"context"
	"os"

	dockerclient "github.com/docker/docker/client"
	"github.com/stretchr/testify/suite"
	"go.uber.org/zap"
	"go.uber.org/zap/zaptest"

	ethcommon "github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/crypto"

	sdkmath "cosmossdk.io/math"

	interchaintest "github.com/strangelove-ventures/interchaintest/v8"
	"github.com/strangelove-ventures/interchaintest/v8/chain/cosmos"
	icethereum "github.com/strangelove-ventures/interchaintest/v8/chain/ethereum"
	"github.com/strangelove-ventures/interchaintest/v8/ibc"
	"github.com/strangelove-ventures/interchaintest/v8/testreporter"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/chainconfig"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/ethereum"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
)

const (
	TestnetTypePoW = "pow"
	TestnetTypePoS = "pos"
)

// TestSuite is a suite of tests that require two chains and a relayer
type TestSuite struct {
	suite.Suite

	ChainA         ethereum.Ethereum
	ethTestnetType string
	ChainB         *cosmos.CosmosChain
	UserB          ibc.Wallet
	dockerClient   *dockerclient.Client
	network        string
	logger         *zap.Logger
	ExecRep        *testreporter.RelayerExecReporter

	EthereumLightClientID         string
	TendermintLightClientID       string
	LastEtheruemLightClientUpdate uint64

	// proposalIDs keeps track of the active proposal ID for cosmos chains
	proposalIDs map[string]uint64
}

// SetupSuite sets up the chains, relayer, user accounts, clients, and connections
func (s *TestSuite) SetupSuite(ctx context.Context) {
	t := s.T()

	icChainSpecs := chainconfig.DefaultChainSpecs

	s.ethTestnetType = os.Getenv(testvalues.EnvKeyEthTestnetType)
	switch s.ethTestnetType {
	case TestnetTypePoW:
		icChainSpecs = append(icChainSpecs, &interchaintest.ChainSpec{ChainConfig: icethereum.DefaultEthereumAnvilChainConfig("ethereum")})
	case TestnetTypePoS:
		kurtosisChain, err := chainconfig.SpinUpKurtosisPoS(ctx) // TODO: Run this in a goroutine and wait for it to be ready
		s.Require().NoError(err)
		s.ChainA, err = ethereum.NewEthereum(ctx, kurtosisChain.RPC, &kurtosisChain.BeaconApiClient, kurtosisChain.Faucet)
		s.Require().NoError(err)
		t.Cleanup(func() {
			ctx := context.Background()
			if t.Failed() {
				_ = kurtosisChain.DumpLogs(ctx)
			}
			kurtosisChain.Destroy(ctx)
		})
	default:
		t.Fatalf("Unknown Ethereum testnet type: %s", s.ethTestnetType)
	}

	s.logger = zaptest.NewLogger(t)
	s.dockerClient, s.network = interchaintest.DockerSetup(t)

	cf := interchaintest.NewBuiltinChainFactory(s.logger, icChainSpecs)

	chains, err := cf.Chains(t.Name())
	s.Require().NoError(err)

	s.ChainB = chains[0].(*cosmos.CosmosChain)
	ic := interchaintest.NewInterchain().
		AddChain(s.ChainB)

	if s.ethTestnetType == TestnetTypePoW {
		anvil := chains[1].(*icethereum.EthereumChain)
		ic = ic.AddChain(anvil)
	}

	s.ExecRep = testreporter.NewNopReporter().RelayerExecReporter(t)

	// TODO: Run this in a goroutine and wait for it to be ready
	s.Require().NoError(ic.Build(ctx, s.ExecRep, interchaintest.InterchainBuildOptions{
		TestName:         t.Name(),
		Client:           s.dockerClient,
		NetworkID:        s.network,
		SkipPathCreation: true,
	}))

	// map all query request types to their gRPC method paths for cosmos chains
	s.Require().NoError(PopulateQueryReqToPath(ctx, s.ChainB))

	if s.ethTestnetType == TestnetTypePoW {
		anvil := chains[1].(*icethereum.EthereumChain)
		faucet, err := crypto.ToECDSA(ethcommon.FromHex("0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"))
		s.Require().NoError(err)

		s.ChainA, err = ethereum.NewEthereum(ctx, anvil.GetHostRPCAddress(), nil, faucet)
		s.Require().NoError(err)
	}

	// Fund user accounts
	cosmosUserFunds := sdkmath.NewInt(testvalues.InitialBalance)
	cosmosUsers := interchaintest.GetAndFundTestUsers(t, ctx, t.Name(), cosmosUserFunds, s.ChainB)
	s.UserB = cosmosUsers[0]

	s.proposalIDs = make(map[string]uint64)
	s.proposalIDs[s.ChainB.Config().ChainID] = 1
}
