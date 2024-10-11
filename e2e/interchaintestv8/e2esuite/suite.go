package e2esuite

import (
	"context"

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
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
)

// TestSuite is a suite of tests that require two chains and a relayer
type TestSuite struct {
	suite.Suite

	ChainA       Ethereum
	ChainB       *cosmos.CosmosChain
	UserB        ibc.Wallet
	dockerClient *dockerclient.Client
	network      string
	logger       *zap.Logger
	ExecRep      *testreporter.RelayerExecReporter

	// proposalIDs keeps track of the active proposal ID for cosmos chains
	proposalIDs map[string]uint64
}

type KurtosisNetworkParams struct {
	Participants []Participant `json:"participants"`
}

type Participant struct {
	CLType        string   `json:"cl_type"`
	CLImage       string   `json:"cl_image"`
	CLExtraParams []string `json:"cl_extra_params"`
	ELImage       string   `json:"el_image"`
	ELLogLevel    string   `json:"el_log_level"`
}

// SetupSuite sets up the chains, relayer, user accounts, clients, and connections
func (s *TestSuite) SetupSuite(ctx context.Context) {
	chainSpecs := chainconfig.DefaultChainSpecs

	if len(chainSpecs) != 1 {
		panic("TestSuite requires exactly 1 chain spec")
	}

	t := s.T()

	s.logger = zaptest.NewLogger(t)
	s.dockerClient, s.network = interchaintest.DockerSetup(t)

	cf := interchaintest.NewBuiltinChainFactory(s.logger, chainSpecs)

	chains, err := cf.Chains(t.Name())
	s.Require().NoError(err)
	s.ChainA, err = SpinUpEthereum(ctx)
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

	// map all query request types to their gRPC method paths for cosmos chains
	s.Require().NoError(PopulateQueryReqToPath(ctx, s.ChainB))

	// Fund user accounts
	cosmosUserFunds := sdkmath.NewInt(testvalues.InitialBalance)
	cosmosUsers := interchaintest.GetAndFundTestUsers(t, ctx, t.Name(), cosmosUserFunds, s.ChainB)
	s.UserB = cosmosUsers[0]

	s.proposalIDs = make(map[string]uint64)
	s.proposalIDs[s.ChainB.Config().ChainID] = 1

	t.Cleanup(func() {
		ctx := context.Background()
		if t.Failed() {
			s.ChainA.DumpLogs(ctx)
		}
		s.ChainA.Destroy(ctx)
	})
}
