package e2esuite

import (
	"context"
	"encoding/json"
	"fmt"

	dockerclient "github.com/docker/docker/client"
	"github.com/kurtosis-tech/kurtosis/api/golang/core/lib/starlark_run_config"
	"github.com/kurtosis-tech/kurtosis/api/golang/engine/lib/kurtosis_context"
	"github.com/stretchr/testify/suite"
	"go.uber.org/zap"
	"go.uber.org/zap/zaptest"

	sdkmath "cosmossdk.io/math"

	interchaintest "github.com/strangelove-ventures/interchaintest/v8"
	"github.com/strangelove-ventures/interchaintest/v8/chain/cosmos"
	"github.com/strangelove-ventures/interchaintest/v8/chain/ethereum"
	"github.com/strangelove-ventures/interchaintest/v8/ibc"
	"github.com/strangelove-ventures/interchaintest/v8/testreporter"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/chainconfig"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
)

// TestSuite is a suite of tests that require two chains and a relayer
type TestSuite struct {
	suite.Suite

	ChainA       *ethereum.EthereumChain
	ChainB       *cosmos.CosmosChain
	UserA        ibc.Wallet
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
}

// SetupSuite sets up the chains, relayer, user accounts, clients, and connections
func (s *TestSuite) SetupSuite(ctx context.Context) {
	chainSpecs := chainconfig.DefaultChainSpecs

	if len(chainSpecs) != 2 {
		panic("TestSuite requires exactly 2 chain specs")
	}

	t := s.T()

	s.logger = zaptest.NewLogger(t)
	s.dockerClient, s.network = interchaintest.DockerSetup(t)

	cf := interchaintest.NewBuiltinChainFactory(s.logger, chainSpecs)

	chains, err := cf.Chains(t.Name())
	s.Require().NoError(err)
	s.ChainA = chains[0].(*ethereum.EthereumChain)
	s.ChainB = chains[1].(*cosmos.CosmosChain)

	s.ExecRep = testreporter.NewNopReporter().RelayerExecReporter(t)

	ic := interchaintest.NewInterchain().
		AddChain(s.ChainA).
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
	ethUsers := interchaintest.GetAndFundTestUsers(t, ctx, t.Name(), testvalues.StartingEthBalance, s.ChainA)
	s.UserA = ethUsers[0]

	s.proposalIDs = make(map[string]uint64)
	s.proposalIDs[s.ChainB.Config().ChainID] = 1

	kurtosisCtx, err := kurtosis_context.NewKurtosisContextFromLocalEngine()
	enclaveName := "my-enclave"
	enclaveCtx, err := kurtosisCtx.CreateEnclave(ctx, enclaveName)
	networkParams := KurtosisNetworkParams{
		Participants: []Participant{
			{
				CLType:        "lighthouse",
				CLImage:       "sigp/lighthouse:latest-unstable",
				CLExtraParams: []string{"--light-client-server"},
			},
		},
	}
	networkParamsJson, err := json.Marshal(networkParams)
	s.Require().NoError(err)
	starlarkResp, err := enclaveCtx.RunStarlarkRemotePackageBlocking(ctx, "github.com/ethpandaops/ethereum-package", &starlark_run_config.StarlarkRunConfig{
		SerializedParams: string(networkParamsJson),
	})
	s.Require().NoError(err)
	fmt.Println(starlarkResp.RunOutput)
	// serviceCtx, err := enclaveCtx.GetServiceContext("my-service")
	// s.Require().NoError(err)

	t.Cleanup(
		func() {
			if err := kurtosisCtx.DestroyEnclave(ctx, enclaveName); err != nil {
				fmt.Printf("Error destroying enclave: %v\n", err)
			}
		},
	)
}
