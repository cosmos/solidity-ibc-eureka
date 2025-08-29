package e2esuite

import (
	"context"
	"os"

	dockerclient "github.com/moby/moby/client"
	"github.com/stretchr/testify/suite"
	"go.uber.org/zap"
	"go.uber.org/zap/zaptest"

	ethcommon "github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/crypto"

	interchaintest "github.com/cosmos/interchaintest/v10"
	"github.com/cosmos/interchaintest/v10/chain/cosmos"
	icfoundry "github.com/cosmos/interchaintest/v10/chain/ethereum/foundry"
	"github.com/cosmos/interchaintest/v10/ibc"
	"github.com/cosmos/interchaintest/v10/testreporter"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/chainconfig"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/ethereum"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
)

const anvilFaucetPrivateKey = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"

// TestSuite is a suite of tests that require two chains and a relayer
type TestSuite struct {
	suite.Suite

	EthChain      ethereum.Ethereum
	OptimismChain chainconfig.KurtosisOptimismChain
	CosmosChains  []*cosmos.CosmosChain
	CosmosUsers   []ibc.Wallet
	dockerClient  *dockerclient.Client
	network       string
	logger        *zap.Logger

	// proposalIDs keeps track of the active proposal ID for cosmos chains
	proposalIDs map[string]uint64
}

// SetupSuite sets up the chains, relayer, user accounts, clients, and connections
func (s *TestSuite) SetupSuite(ctx context.Context) {
	icChainSpecs := chainconfig.DefaultChainSpecs

	s.logger = zaptest.NewLogger(s.T())
	ethTestnetType := os.Getenv(testvalues.EnvKeyEthTestnetType)
	switch ethTestnetType {
	case testvalues.EthTestnetTypePoW:
		icChainSpecs = append(icChainSpecs, &interchaintest.ChainSpec{ChainConfig: icfoundry.DefaultEthereumAnvilChainConfig("ethereum")})
	case testvalues.EthTestnetTypePoS:
		kurtosisEthChain, err := chainconfig.SpinUpKurtosisEthPoS(ctx) // TODO: Run this in a goroutine and wait for it to be ready
		s.Require().NoError(err)
		s.EthChain, err = ethereum.NewEthereum(ctx, kurtosisEthChain.RPC, &kurtosisEthChain.BeaconApiClient, kurtosisEthChain.Faucet)
		s.Require().NoError(err)
		s.T().Cleanup(func() {
			ctx := context.Background()
			if s.T().Failed() {
				_ = kurtosisEthChain.DumpLogs(ctx)
			}
			kurtosisEthChain.Destroy(ctx)
		})
	case testvalues.EthTestnetTypeOptimism:
		kurtosisOptimismChain, err := chainconfig.SpinUpKurtosisOptimism(ctx) // TODO: Run this in a goroutine and wait for it to be ready
		s.OptimismChain = kurtosisOptimismChain
		s.Require().NoError(err)
		s.EthChain, err = ethereum.NewEthereum(ctx, kurtosisOptimismChain.ExecutionRPC, nil, kurtosisOptimismChain.Faucet)
		s.Require().NoError(err)
		s.T().Cleanup(func() {
			ctx := context.Background()
			if s.T().Failed() {
				_ = kurtosisOptimismChain.DumpLogs(ctx)
			}
			kurtosisOptimismChain.Destroy(ctx)
		})
	case testvalues.EthTestnetTypeArbitrum:
		arbitrumChain, err := chainconfig.SpinUpTestnodeArbitrum(ctx) // TODO: Run this in a goroutine and wait for it to be ready
		s.Require().NoError(err)
		s.EthChain, err = ethereum.NewEthereum(ctx, arbitrumChain.ExecutionRPC, nil, nil) // No faucet for Arbitrum testnode yet
		s.Require().NoError(err)
		s.T().Cleanup(func() {
			ctx := context.Background()
			if s.T().Failed() {
				_ = arbitrumChain.DumpLogs(ctx)
			}
			arbitrumChain.Destroy(ctx)
		})
	case testvalues.EthTestnetTypeNone:
		// Do nothing
	default:
		s.T().Fatalf("Unknown Ethereum testnet type: %s", ethTestnetType)
	}

	s.dockerClient, s.network = interchaintest.DockerSetup(s.T())

	cf := interchaintest.NewBuiltinChainFactory(s.logger, icChainSpecs)

	chains, err := cf.Chains(s.T().Name())
	s.Require().NoError(err)

	ic := interchaintest.NewInterchain()
	for _, chain := range chains {
		ic = ic.AddChain(chain)
	}

	execRep := testreporter.NewNopReporter().RelayerExecReporter(s.T())

	// TODO: Run this in a goroutine and wait for it to be ready
	s.Require().NoError(ic.Build(ctx, execRep, interchaintest.InterchainBuildOptions{
		TestName:         s.T().Name(),
		Client:           s.dockerClient,
		NetworkID:        s.network,
		SkipPathCreation: true,
	}))

	if ethTestnetType == testvalues.EthTestnetTypePoW {
		anvil := chains[len(chains)-1].(*icfoundry.AnvilChain)
		faucet, err := crypto.ToECDSA(ethcommon.FromHex(anvilFaucetPrivateKey))
		s.Require().NoError(err)

		s.EthChain, err = ethereum.NewEthereum(ctx, anvil.GetHostRPCAddress(), nil, faucet)
		s.Require().NoError(err)

		// Remove the Ethereum chain from the cosmos chains
		chains = chains[:len(chains)-1]
	}

	for _, chain := range chains {
		cosmosChain := chain.(*cosmos.CosmosChain)
		s.CosmosChains = append(s.CosmosChains, cosmosChain)
	}

	// map all query request types to their gRPC method paths for cosmos chains
	s.Require().NoError(populateQueryReqToPath(ctx, s.CosmosChains[0]))

	// Fund user accounts
	for _, chain := range chains {
		s.CosmosUsers = append(s.CosmosUsers, s.CreateAndFundCosmosUser(ctx, chain.(*cosmos.CosmosChain)))
	}

	s.proposalIDs = make(map[string]uint64)
	for _, chain := range s.CosmosChains {
		s.proposalIDs[chain.Config().ChainID] = 1
	}
}
