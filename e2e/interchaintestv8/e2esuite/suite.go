package e2esuite

import (
	"context"
	"fmt"
	"os"
	"strconv"
	"strings"

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

const (
	anvilFaucetPrivateKey = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
	baseAnvilChainID      = 31337
)

// TestSuite is a suite of tests that require two chains and a relayer
type TestSuite struct {
	suite.Suite

	EthChains      []*ethereum.Ethereum
	ethTestnetType string
	EthWasmType    string
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

	// AnvilCount specifies how many Anvil chains to create. Only used when ETH_TESTNET_TYPE=pow.
	AnvilCount int
}

// Chain-specific configuration types

type ethereumConfig struct {
	testnetType string
	anvilCount  int
}

func (c *ethereumConfig) isAnvilBased() bool {
	return c.testnetType == testvalues.EthTestnetTypePoW ||
		c.testnetType == testvalues.EthTestnetTypeOptimism ||
		c.testnetType == testvalues.EthTestnetTypeArbitrum
}

func (c *ethereumConfig) needsPoS() bool {
	return c.testnetType == testvalues.EthTestnetTypePoS
}

type solanaConfig struct {
	testnetType string
}

// cosmosLightClientConfig holds config for wasm light clients deployed on Cosmos
type cosmosLightClientConfig struct {
	ethWasmType        string // dummy, full, or attestor
	wasmLightClientTag string // version tag or "local"
}

// setupConfig holds parsed configuration for all chain types
type setupConfig struct {
	ethereum          ethereumConfig
	solana            solanaConfig
	cosmosLightClient cosmosLightClientConfig
}

// Result types for parallel chain setup
type solanaSetupResult struct {
	chain *chainconfig.SolanaLocalnetChain
	err   error
}

type interchainSetupResult struct {
	chains       []ibc.Chain
	dockerClient *dockerclient.Client
	network      string
	logger       *zap.Logger
	err          error
}

type ethPosSetupResult struct {
	chain *chainconfig.EthKurtosisChain
	err   error
}

// SetupSuite sets up the chains, relayer, user accounts, clients, and connections
func (s *TestSuite) SetupSuite(ctx context.Context) {
	cfg := s.parseConfig()
	s.ethTestnetType = cfg.ethereum.testnetType
	s.EthWasmType = cfg.cosmosLightClient.ethWasmType
	s.WasmLightClientTag = cfg.cosmosLightClient.wasmLightClientTag

	if cfg.cosmosLightClient.ethWasmType != "" {
		s.T().Logf("wasm type %s", cfg.cosmosLightClient.ethWasmType)
	}

	chainSpecs := s.buildChainSpecs(cfg)
	chains, dockerClient, network, logger := s.setupChainsInParallel(ctx, cfg, chainSpecs)

	s.logger = logger
	s.dockerClient = dockerClient
	s.network = network

	s.processChains(ctx, cfg, chains)
}

// GetDockerClient returns the Docker client used for container management.
// This is primarily used for setting up Docker-based attestors.
func (s *TestSuite) GetDockerClient() *dockerclient.Client {
	return s.dockerClient
}

// GetNetworkID returns the Docker network ID used for container networking.
// This is primarily used for setting up Docker-based attestors.
func (s *TestSuite) GetNetworkID() string {
	return s.network
}

// SetupDocker initializes just the Docker client and network without starting any chains.
// This is useful for lightweight tests that only need Docker resources.
func (s *TestSuite) SetupDocker() {
	if s.dockerClient != nil {
		return // Already initialized
	}
	s.dockerClient, s.network = interchaintest.DockerSetup(s.T())
}

// AddAnvilChain creates and adds a new Anvil chain to EthChains.
// Returns the index of the newly added chain.
func (s *TestSuite) AddAnvilChain(ctx context.Context, rpcAddress string) (int, error) {
	faucet, err := crypto.ToECDSA(ethcommon.FromHex(anvilFaucetPrivateKey))
	if err != nil {
		return -1, err
	}

	ethChain, err := ethereum.NewEthereum(ctx, rpcAddress, nil, faucet)
	if err != nil {
		return -1, err
	}

	s.EthChains = append(s.EthChains, &ethChain)
	return len(s.EthChains) - 1, nil
}

func (s *TestSuite) parseConfig() setupConfig {
	cfg := setupConfig{
		ethereum: ethereumConfig{
			testnetType: os.Getenv(testvalues.EnvKeyEthTestnetType),
		},
		solana: solanaConfig{
			testnetType: os.Getenv(testvalues.EnvKeySolanaTestnetType),
		},
	}

	if s.WasmLightClientTag != "" {
		cfg.cosmosLightClient.wasmLightClientTag = s.WasmLightClientTag
	} else {
		cfg.cosmosLightClient.wasmLightClientTag = os.Getenv(testvalues.EnvKeyE2EWasmLightClientTag)
	}

	if s.EthWasmType != "" {
		cfg.cosmosLightClient.ethWasmType = s.EthWasmType
	} else {
		cfg.cosmosLightClient.ethWasmType = os.Getenv(testvalues.EnvKeyEthLcOnCosmos)
	}

	if cfg.ethereum.isAnvilBased() {
		cfg.ethereum.anvilCount = 1
		if s.AnvilCount > 0 {
			cfg.ethereum.anvilCount = s.AnvilCount
		}
	}

	return cfg
}

func (s *TestSuite) buildChainSpecs(cfg setupConfig) []*interchaintest.ChainSpec {
	if !cfg.ethereum.isAnvilBased() {
		return chainconfig.DefaultChainSpecs
	}

	// Multi-anvil: only Anvil chains, no Cosmos
	if cfg.ethereum.anvilCount > 1 {
		return s.buildAnvilSpecs(cfg.ethereum.anvilCount)
	}

	// Single anvil: Cosmos chains + one Anvil
	specs := chainconfig.DefaultChainSpecs
	specs = append(specs, &interchaintest.ChainSpec{
		ChainConfig: icfoundry.DefaultEthereumAnvilChainConfig("ethereum"),
	})
	return specs
}

func (s *TestSuite) buildAnvilSpecs(count int) []*interchaintest.ChainSpec {
	specs := make([]*interchaintest.ChainSpec, 0, count)
	for i := 0; i < count; i++ {
		chainID := strconv.Itoa(baseAnvilChainID + i)
		name := fmt.Sprintf("ethereum-%d", i)
		chainConfig := icfoundry.DefaultEthereumAnvilChainConfig(name)
		chainConfig.ChainID = chainID
		chainConfig.AdditionalStartArgs = append(chainConfig.AdditionalStartArgs, "--chain-id", chainID)
		specs = append(specs, &interchaintest.ChainSpec{ChainConfig: chainConfig})
	}
	return specs
}

func (s *TestSuite) setupChainsInParallel(
	ctx context.Context,
	cfg setupConfig,
	chainSpecs []*interchaintest.ChainSpec,
) ([]ibc.Chain, *dockerclient.Client, string, *zap.Logger) {
	solanaCh := make(chan solanaSetupResult, 1)
	interchainCh := make(chan interchainSetupResult, 1)
	ethPosCh := make(chan ethPosSetupResult, 1)

	go s.setupSolanaAsync(ctx, cfg, solanaCh)
	go s.setupInterchainAsync(ctx, chainSpecs, interchainCh)

	if cfg.ethereum.needsPoS() {
		go s.setupEthereumPoSAsync(ctx, ethPosCh)
	} else {
		ethPosCh <- ethPosSetupResult{}
	}

	solanaRes := <-solanaCh
	s.Require().NoError(solanaRes.err, "Solana chain setup failed")
	s.processSolanaResult(solanaRes.chain)

	interchainRes := <-interchainCh
	s.Require().NoError(interchainRes.err, "Interchain setup failed")

	ethPosRes := <-ethPosCh
	s.Require().NoError(ethPosRes.err, "Ethereum PoS chain setup failed")
	s.processEthereumPoSResult(ctx, ethPosRes.chain)

	return interchainRes.chains, interchainRes.dockerClient, interchainRes.network, interchainRes.logger
}

func (s *TestSuite) setupSolanaAsync(ctx context.Context, cfg setupConfig, resultCh chan<- solanaSetupResult) {
	var result solanaSetupResult
	defer func() { resultCh <- result }()

	switch cfg.solana.testnetType {
	case testvalues.SolanaTestnetType_Localnet:
		chain, err := chainconfig.StartLocalnet(ctx)
		if err != nil {
			result.err = err
			return
		}
		result.chain = &chain
	case testvalues.SolanaTestnetType_None, "":
		// No Solana
	default:
		result.err = fmt.Errorf("unknown Solana testnet type: %s", cfg.solana.testnetType)
	}
}

func (s *TestSuite) setupInterchainAsync(
	ctx context.Context,
	chainSpecs []*interchaintest.ChainSpec,
	resultCh chan<- interchainSetupResult,
) {
	var result interchainSetupResult
	defer func() { resultCh <- result }()

	testName := s.T().Name()
	logger := zaptest.NewLogger(s.T())
	dockerClient, network := interchaintest.DockerSetup(s.T())

	result.logger = logger
	result.dockerClient = dockerClient
	result.network = network

	cf := interchaintest.NewBuiltinChainFactory(logger, chainSpecs)
	chains, err := cf.Chains(testName)
	if err != nil {
		result.err = err
		return
	}

	ic := interchaintest.NewInterchain()
	for _, chain := range chains {
		ic = ic.AddChain(chain)
	}

	execRep := testreporter.NewNopReporter().RelayerExecReporter(s.T())
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
}

func (s *TestSuite) setupEthereumPoSAsync(ctx context.Context, resultCh chan<- ethPosSetupResult) {
	var result ethPosSetupResult
	defer func() { resultCh <- result }()

	chain, err := chainconfig.SpinUpKurtosisPoS(ctx)
	if err != nil {
		result.err = err
		return
	}
	result.chain = &chain
}

func (s *TestSuite) processSolanaResult(chain *chainconfig.SolanaLocalnetChain) {
	if chain == nil {
		return
	}

	s.T().Cleanup(func() {
		if err := chain.Destroy(); err != nil {
			s.T().Logf("Failed to destroy Solana localnet: %v", err)
		}
	})

	var err error
	s.SolanaChain, err = solana.NewLocalnetSolana(chain.Faucet)
	s.Require().NoError(err, "Failed to create Solana client")
}

func (s *TestSuite) processEthereumPoSResult(ctx context.Context, chain *chainconfig.EthKurtosisChain) {
	if chain == nil {
		return
	}

	s.T().Cleanup(func() {
		cleanupCtx := context.Background()
		if s.T().Failed() {
			_ = chain.DumpLogs(cleanupCtx)
		}
		chain.Destroy(cleanupCtx)
	})

	ethChain, err := ethereum.NewEthereum(ctx, chain.RPC, &chain.BeaconApiClient, chain.Faucet)
	s.Require().NoError(err, "Failed to create Ethereum client")
	s.EthChains = append(s.EthChains, &ethChain)
}

func (s *TestSuite) processChains(ctx context.Context, cfg setupConfig, chains []ibc.Chain) {
	cosmosChains := s.setupAnvilChains(ctx, chains, cfg.ethereum.anvilCount)
	s.setupCosmosChains(ctx, cosmosChains)
}

// setupAnvilChains sets up Anvil chains from the end of the chains slice.
// Returns the remaining (non-Anvil) chains.
func (s *TestSuite) setupAnvilChains(ctx context.Context, chains []ibc.Chain, count int) []ibc.Chain {
	if count == 0 {
		return chains
	}

	faucet, err := crypto.ToECDSA(ethcommon.FromHex(anvilFaucetPrivateKey))
	s.Require().NoError(err)

	anvilStartIdx := len(chains) - count
	for i := anvilStartIdx; i < len(chains); i++ {
		anvil := chains[i].(*icfoundry.AnvilChain)
		rpcAddr := strings.Replace(anvil.GetHostRPCAddress(), "0.0.0.0", "127.0.0.1", 1)
		ethChain, err := ethereum.NewEthereum(ctx, rpcAddr, nil, faucet)
		s.Require().NoError(err)
		// Store Docker internal RPC address for container-to-container communication
		ethChain.DockerRPC = anvil.GetRPCAddress()
		s.EthChains = append(s.EthChains, &ethChain)
	}
	return chains[:anvilStartIdx]
}

func (s *TestSuite) setupCosmosChains(ctx context.Context, chains []ibc.Chain) {
	if len(chains) == 0 {
		return
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
