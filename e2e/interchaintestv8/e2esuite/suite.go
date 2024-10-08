package e2esuite

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"time"

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

	GethRPC   string
	BeaconRPC string
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
	s.Require().NoError(err)

	enclaveName := "my-enclave"
	enclaves, err := kurtosisCtx.GetEnclaves(ctx)
	s.Require().NoError(err)

	if enclaveInfos, found := enclaves.GetEnclavesByName()[enclaveName]; found {
		for _, enclaveInfo := range enclaveInfos {
			err = kurtosisCtx.DestroyEnclave(ctx, enclaveInfo.EnclaveUuid)
			s.Require().NoError(err)
		}
	}
	enclaveCtx, err := kurtosisCtx.CreateEnclave(ctx, enclaveName)
	s.Require().NoError(err)
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

	time.Sleep(2 * time.Minute)

	gethCtx, err := enclaveCtx.GetServiceContext("el-1-geth-lighthouse")
	s.Require().NoError(err)
	rpcPortSpec := gethCtx.GetPublicPorts()["rpc"]
	s.GethRPC = fmt.Sprintf("http://localhost:%d", rpcPortSpec.GetNumber())

	lighthouseCtx, err := enclaveCtx.GetServiceContext("cl-1-lighthouse-geth")
	s.Require().NoError(err)
	beaconPortSpec := lighthouseCtx.GetPublicPorts()["http"]
	s.BeaconRPC = fmt.Sprintf("http://localhost:%d", beaconPortSpec.GetNumber())

	cmd := exec.Command("forge", "script", "--rpc-url", s.GethRPC, "--broadcast", "--slow", "--delay", "30", "--retries", "20", "-vvvv", "../../script/E2ETestDeploy.s.sol:E2ETestDeploy")
	cmd.Env = append(cmd.Env, "PRIVATE_KEY=0x4b9f63ecf84210c5366c66d68fa1f5da1fa4f634fad6dfc86178e4d79ff9e59")
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	err = cmd.Run()
	// if err != nil {
	// 	// forge script sometimes fail waiting for tx indexing
	// 	cmd.Args = append(cmd.Args, "--resume")
	// 	err = cmd.Run()
	// 	s.Require().NoError(err)
	// }
	s.Require().NoError(err)
	//PRIVATE_KEY=0x4b9f63ecf84210c5366c66d68fa1f5da1fa4f634fad6dfc86178e4d79ff9e59 forge script script/E2ETestDeploy.s.sol:E2ETestDeploy --rpc-url http://localhost:32856 --broadcast -vvvv

	t.Cleanup(
		func() {
			if err := kurtosisCtx.DestroyEnclave(ctx, enclaveName); err != nil {
				fmt.Printf("Error destroying enclave: %v\n", err)
			}
		},
	)
}
