package e2esuite

import (
	"context"
	"fmt"

	dockerclient "github.com/docker/docker/client"

	"github.com/stretchr/testify/suite"
	"go.uber.org/zap"
	"go.uber.org/zap/zaptest"

	sdkmath "cosmossdk.io/math"

	interchaintest "github.com/strangelove-ventures/interchaintest/v8"
	"github.com/strangelove-ventures/interchaintest/v8/chain/cosmos"
	"github.com/strangelove-ventures/interchaintest/v8/ibc"
	"github.com/strangelove-ventures/interchaintest/v8/testreporter"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/chainconfig"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/ethereum"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/visualizerclient"
)

const visualizerPort = 6969

// TestSuite is a suite of tests that require two chains and a relayer
type TestSuite struct {
	suite.Suite

	ChainA           ethereum.Ethereum
	ChainB           *cosmos.CosmosChain
	UserB            ibc.Wallet
	dockerClient     *dockerclient.Client
	network          string
	logger           *zap.Logger
	ExecRep          *testreporter.RelayerExecReporter
	VisualizerClient *visualizerclient.VisualizerClient

	// proposalIDs keeps track of the active proposal ID for cosmos chains
	proposalIDs map[string]uint64
}

// SetupSuite sets up the chains, relayer, user accounts, clients, and connections
func (s *TestSuite) SetupSuite(ctx context.Context) {
	t := s.T()

	s.VisualizerClient = visualizerclient.NewVisualizerClient(visualizerPort, t.Name())
	s.VisualizerClient.SendMessage("TestSuite setup started")
	chainSpecs := chainconfig.DefaultChainSpecs

	t.Cleanup(func() {
		ctx := context.Background()
		if t.Failed() {
			s.VisualizerClient.SendMessage("Test failed")
			s.ChainA.DumpLogs(ctx)
		}
		s.ChainA.Destroy(ctx)
		s.VisualizerClient.SendMessage("Test run done and cleanup completed")
	})

	if len(chainSpecs) != 1 {
		t.Fatal("TestSuite requires exactly 1 chain spec")
	}

	s.logger = zaptest.NewLogger(t)
	s.dockerClient, s.network = interchaintest.DockerSetup(t)

	cf := interchaintest.NewBuiltinChainFactory(s.logger, chainSpecs)

	chains, err := cf.Chains(t.Name())
	s.Require().NoError(err)
	s.ChainA, err = ethereum.SpinUpEthereum(ctx)
	s.Require().NoError(err)
	s.ChainB = chains[0].(*cosmos.CosmosChain)

	s.ExecRep = testreporter.NewNopReporter().RelayerExecReporter(t)

	ic := interchaintest.NewInterchain().
		AddChain(s.ChainB)

	s.Require().NoError(ic.Build(ctx, s.ExecRep, interchaintest.InterchainBuildOptions{
		TestName:         t.Name(),
		Client:           s.dockerClient,
		NetworkID:        s.network,
		SkipPathCreation: true,
	}))

	s.VisualizerClient.SendMessage(fmt.Sprintf("Chains started: %s, %s", s.ChainA.ChainID.String(), s.ChainB.Config().ChainID))

	// map all query request types to their gRPC method paths for cosmos chains
	s.Require().NoError(PopulateQueryReqToPath(ctx, s.ChainB))

	// Fund user accounts
	cosmosUserFunds := sdkmath.NewInt(testvalues.InitialBalance)
	cosmosUsers := interchaintest.GetAndFundTestUsers(t, ctx, t.Name(), cosmosUserFunds, s.ChainB)
	s.UserB = cosmosUsers[0]

	s.proposalIDs = make(map[string]uint64)
	s.proposalIDs[s.ChainB.Config().ChainID] = 1

}
