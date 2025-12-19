package e2esuite

import (
	"context"
	"fmt"
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
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/solana"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
)

const anvilFaucetPrivateKey = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"

// TestSuite is a suite of tests that require two chains and a relayer
type TestSuite struct {
	suite.Suite

	EthChain       ethereum.Ethereum
	ethTestnetType string
	CosmosChains   []*cosmos.CosmosChain
	CosmosUsers    []ibc.Wallet
	SolanaChain    solana.Solana
	dockerClient   *dockerclient.Client
	network        string
	logger         *zap.Logger

	// proposalIDs keeps track of the active proposal ID for cosmos chains
	proposalIDs map[string]uint64
	// WasmLightClientTag decides which version of the eth light client to use.
	// Either an empty string, or 'local', means it will use the local binary in the repo, unless running in mock mode
	// otherwise, it will download the version from the github release with the given tag
	WasmLightClientTag string
}

// SetupSuite sets up the chains, relayer, user accounts, clients, and connections
func (s *TestSuite) SetupSuite(ctx context.Context) {
	// To let the download version be overridden by a calling test
	if s.WasmLightClientTag == "" {
		s.WasmLightClientTag = os.Getenv(testvalues.EnvKeyE2EWasmLightClientTag)
	}

	icChainSpecs := chainconfig.DefaultChainSpecs

	// Handle Ethereum PoW case first (modifies icChainSpecs needed by Cosmos setup)
	s.ethTestnetType = os.Getenv(testvalues.EnvKeyEthTestnetType)
	if s.ethTestnetType == testvalues.EthTestnetTypePoW {
		icChainSpecs = append(icChainSpecs, &interchaintest.ChainSpec{ChainConfig: icfoundry.DefaultEthereumAnvilChainConfig("ethereum")})
	}

	// Parallelize Solana, Cosmos, and Ethereum PoS chain setup for faster initialization
	type solanaSetupResult struct {
		solanaChain *chainconfig.SolanaLocalnetChain
		err         error
	}

	type cosmosSetupResult struct {
		chains       []ibc.Chain
		logger       *zap.Logger
		dockerClient *dockerclient.Client
		network      string
		err          error
	}

	type ethSetupResult struct {
		ethChain *chainconfig.EthKurtosisChain
		err      error
	}

	solanaResults := make(chan solanaSetupResult, 1)
	cosmosResults := make(chan cosmosSetupResult, 1)
	ethResults := make(chan ethSetupResult, 1)

	solanaTestnetType := os.Getenv(testvalues.EnvKeySolanaTestnetType)

	// Setup Solana in parallel
	go func() {
		var result solanaSetupResult
		defer func() {
			solanaResults <- result
		}()

		switch solanaTestnetType {
		case testvalues.SolanaTestnetType_Localnet:
			solChain, err := chainconfig.StartLocalnet(ctx)
			if err != nil {
				result.err = err
				return
			}
			result.solanaChain = &solChain
		case testvalues.SolanaTestnetType_None, "":
			// Do nothing
		default:
			result.err = fmt.Errorf("unknown Solana testnet type: %s", solanaTestnetType)
		}
	}()

	// Setup Cosmos chains in parallel
	testName := s.T().Name()
	testInstance := s.T()
	go func() {
		var result cosmosSetupResult
		defer func() {
			cosmosResults <- result
		}()

		logger := zaptest.NewLogger(testInstance)
		dockerClient, network := interchaintest.DockerSetup(testInstance)

		result.logger = logger
		result.dockerClient = dockerClient
		result.network = network

		cf := interchaintest.NewBuiltinChainFactory(logger, icChainSpecs)

		chains, err := cf.Chains(testName)
		if err != nil {
			result.err = err
			return
		}

		ic := interchaintest.NewInterchain()
		for _, chain := range chains {
			ic = ic.AddChain(chain)
		}

		execRep := testreporter.NewNopReporter().RelayerExecReporter(testInstance)

		err = ic.Build(ctx, execRep, interchaintest.InterchainBuildOptions{
			TestName:         testName,
			Client:           dockerClient,
			NetworkID:        network,
			SkipPathCreation: true,
		})
		if err != nil {
			result.err = err
			return
		}

		result.chains = chains
	}()

	// Setup Ethereum PoS in parallel (if enabled)
	ethEnabled := s.ethTestnetType == testvalues.EthTestnetTypePoS
	if ethEnabled {
		go func() {
			var result ethSetupResult
			defer func() {
				ethResults <- result
			}()

			kurtosisChain, err := chainconfig.SpinUpKurtosisPoS(ctx)
			if err != nil {
				result.err = err
				return
			}
			result.ethChain = &kurtosisChain
		}()
	}

	// Wait for Solana setup to complete
	solanaResult := <-solanaResults
	s.Require().NoError(solanaResult.err, "Solana chain setup failed")
	if solanaResult.solanaChain != nil {
		solChain := solanaResult.solanaChain
		s.T().Cleanup(func() {
			if err := solChain.Destroy(); err != nil {
				s.T().Logf("Failed to destroy Solana localnet: %v", err)
			}
		})
		var err error
		s.SolanaChain, err = solana.NewLocalnetSolana(solChain.Faucet)
		s.Require().NoError(err, "Failed to create Solana client")
	}

	// Wait for Cosmos setup to complete
	cosmosResult := <-cosmosResults
	s.Require().NoError(cosmosResult.err, "Cosmos chain setup failed")
	s.logger = cosmosResult.logger
	s.dockerClient = cosmosResult.dockerClient
	s.network = cosmosResult.network
	var chains []ibc.Chain
	if cosmosResult.chains != nil {
		chains = cosmosResult.chains
	}

	// Wait for Ethereum PoS setup if enabled
	if ethEnabled {
		ethResult := <-ethResults
		s.Require().NoError(ethResult.err, "Ethereum PoS chain setup failed")
		if ethResult.ethChain != nil {
			kurtosisChain := ethResult.ethChain
			s.T().Cleanup(func() {
				ctx := context.Background()
				if s.T().Failed() {
					_ = kurtosisChain.DumpLogs(ctx)
				}
				kurtosisChain.Destroy(ctx)
			})
			var err error
			s.EthChain, err = ethereum.NewEthereum(ctx, kurtosisChain.RPC, &kurtosisChain.BeaconApiClient, kurtosisChain.Faucet)
			s.Require().NoError(err, "Failed to create Ethereum client")
		}
	}

	// Handle remaining Ethereum cases
	switch s.ethTestnetType {
	case testvalues.EthTestnetTypePoW:
		icChainSpecs = append(icChainSpecs, &interchaintest.ChainSpec{ChainConfig: icfoundry.DefaultEthereumAnvilChainConfig("ethereum")})
	case testvalues.EthTestnetTypePoS:
		// Already setup in parallel goroutine above
	case testvalues.EthTestnetTypeNone:
		// Do nothing
	default:
		s.T().Fatalf("Unknown Ethereum testnet type: %s", s.ethTestnetType)
	}

	if s.ethTestnetType == testvalues.EthTestnetTypePoW {
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
