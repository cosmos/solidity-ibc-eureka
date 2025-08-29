package chainconfig

import (
	"context"
	"crypto/ecdsa"
	"fmt"
	"os"
	"time"

	ethcommon "github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/crypto"

	"github.com/cosmos/interchaintest/v10/testutil"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/ethereum"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
)

const (
	// ethereumPackageId is the package ID used by Kurtosis to find the Ethereum package we use for the testnet
	ethereumPackageId = "github.com/ethpandaops/ethereum-package@5.0.1"

	ethFaucetPrivateKey = "0x04b9f63ecf84210c5366c66d68fa1f5da1fa4f634fad6dfc86178e4d79ff9e59"
)

type EthKurtosisChain struct {
	RPC             string
	BeaconApiClient ethereum.BeaconAPIClient
	Faucet          *ecdsa.PrivateKey

	kurtosisEnclave  KurtosisEnclave
	executionService string
	consensusService string
}

// To see all the configuration options: github.com/ethpandaops/ethereum-package
type kurtosisEthNetworkParams struct {
	Participants        []kurtosisEthParticipant       `json:"participants"`
	NetworkParams       kurtosisEthNetworkConfigParams `json:"network_params"`
	WaitForFinalization bool                           `json:"wait_for_finalization"`
	AdditionalServices  []string                       `json:"additional_services"`
}

type kurtosisEthParticipant struct {
	CLType         string   `json:"cl_type"`
	CLImage        string   `json:"cl_image"`
	ELType         string   `json:"el_type"`
	ELImage        string   `json:"el_image"`
	ELExtraParams  []string `json:"el_extra_params"`
	ELLogLevel     string   `json:"el_log_level"`
	ValidatorCount uint64   `json:"validator_count"`
}

type kurtosisEthNetworkConfigParams struct {
	Preset           string `json:"preset"`
	ElectraForkEpoch uint64 `json:"electra_fork_epoch"`
}

// getKurtosisPreset returns the Kurtosis preset to use.
// It retrieves the preset from the environment variable or falls back to the default.
func getKurtosisPreset() string {
	preset := os.Getenv(testvalues.EnvKeyEthereumPosNetworkPreset)
	if preset == "" {
		return testvalues.EnvValueEthereumPosPreset_Minimal
	}
	return preset
}

func SpinUpKurtosisEthPoS(ctx context.Context) (EthKurtosisChain, error) {
	ethNetworkParams := kurtosisEthNetworkParams{
		Participants: []kurtosisEthParticipant{
			{
				CLType:         "lodestar",
				CLImage:        "chainsafe/lodestar:v1.33.0",
				ELType:         "geth",
				ELImage:        "ethereum/client-go:v1.16.2",
				ELExtraParams:  []string{"--gcmode=archive"},
				ELLogLevel:     "info",
				ValidatorCount: 64,
			},
		},
		// We
		NetworkParams: kurtosisEthNetworkConfigParams{
			Preset:           "minimal",
			ElectraForkEpoch: 1,
		},
		WaitForFinalization: true,
		AdditionalServices:  []string{},
	}
	executionService := fmt.Sprintf("el-1-%s-%s", ethNetworkParams.Participants[0].ELType, ethNetworkParams.Participants[0].CLType)
	consensusService := fmt.Sprintf("cl-1-%s-%s", ethNetworkParams.Participants[0].CLType, ethNetworkParams.Participants[0].ELType)

	// Load dynamic configurations
	ethNetworkParams.NetworkParams.Preset = getKurtosisPreset()
	faucet, err := crypto.ToECDSA(ethcommon.FromHex(ethFaucetPrivateKey))
	if err != nil {
		return EthKurtosisChain{}, err
	}

	kurtosisEnclave, err := spinUpKurtosisEnclave(ctx, "ethereum-pos-testnet", ethereumPackageId, ethNetworkParams)
	if err != nil {
		return EthKurtosisChain{}, fmt.Errorf("failed to spin up Kurtosis enclave: %w", err)
	}

	// exeuctionCtx is the service context (kurtosis concept) for the execution node that allows us to get the public ports
	executionCtx, err := kurtosisEnclave.enclaveCtx.GetServiceContext(executionService)
	if err != nil {
		return EthKurtosisChain{}, err
	}
	rpcPortSpec := executionCtx.GetPublicPorts()["rpc"]
	rpc := fmt.Sprintf("http://localhost:%d", rpcPortSpec.GetNumber())

	// consensusCtx is the service context (kurtosis concept) for the consensus node that allows us to get the public ports
	consensusCtx, err := kurtosisEnclave.enclaveCtx.GetServiceContext(consensusService)
	if err != nil {
		return EthKurtosisChain{}, err
	}
	beaconPortSpec := consensusCtx.GetPublicPorts()["http"]
	beaconRPC := fmt.Sprintf("http://localhost:%d", beaconPortSpec.GetNumber())

	// Wait for the chain to finalize
	var beaconAPIClient ethereum.BeaconAPIClient
	err = testutil.WaitForCondition(30*time.Minute, 5*time.Second, func() (bool, error) {
		beaconAPIClient, err = ethereum.NewBeaconAPIClient(ctx, beaconRPC)
		if err != nil {
			return false, nil
		}

		finalizedBlocksResp, err := beaconAPIClient.GetFinalizedBlocks()
		fmt.Printf("Waiting for chain to finalize, finalizedBlockResp: %+v, err: %s\n", finalizedBlocksResp, err)
		if err != nil {
			return false, nil
		}
		if !finalizedBlocksResp.Finalized {
			return false, nil
		}

		header, err := beaconAPIClient.GetHeader(finalizedBlocksResp.Data.Message.Slot)
		if err != nil {
			return false, nil
		}
		bootstrap, err := beaconAPIClient.GetBootstrap(header.Root)
		if err != nil {
			return false, nil
		}

		return bootstrap.Data.Header.Beacon.Slot != 0, nil
	})
	if err != nil {
		return EthKurtosisChain{}, err
	}

	return EthKurtosisChain{
		RPC:             rpc,
		BeaconApiClient: beaconAPIClient,
		Faucet:          faucet,

		kurtosisEnclave:  kurtosisEnclave,
		executionService: executionService,
		consensusService: consensusService,
	}, nil
}

func (e EthKurtosisChain) DumpLogs(ctx context.Context) error {
	return e.kurtosisEnclave.DumpLogs(ctx, e.executionService)
}

func (e EthKurtosisChain) Destroy(ctx context.Context) {
	e.kurtosisEnclave.Destroy(ctx)
}
