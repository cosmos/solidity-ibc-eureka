package chainconfig

import (
	"context"
	"encoding/json"
	"fmt"

	"github.com/kurtosis-tech/kurtosis/api/golang/core/lib/enclaves"
	"github.com/kurtosis-tech/kurtosis/api/golang/core/lib/services"
	"github.com/kurtosis-tech/kurtosis/api/golang/core/lib/starlark_run_config"
	"github.com/kurtosis-tech/kurtosis/api/golang/engine/lib/kurtosis_context"
)

const (
	// ethereumPackageId is the package ID used by Kurtosis to find the Ethereum package we use for the testnet
	ethereumPackageId = "github.com/ethpandaops/ethereum-package@5.0.1"

	faucetPrivateKey = "0x04b9f63ecf84210c5366c66d68fa1f5da1fa4f634fad6dfc86178e4d79ff9e59"
)

var (
	// KurtosisConfig sets up the default values for the eth testnet
	// It can be changed before calling SetupSuite to alter the testnet configuration
	KurtosisConfig = kurtosisNetworkParams{
		Participants: []kurtosisParticipant{
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
		NetworkParams: kurtosisNetworkConfigParams{
			Preset:           "minimal",
			ElectraForkEpoch: 1,
		},
		WaitForFinalization: true,
		AdditionalServices:  []string{},
	}
	executionService = fmt.Sprintf("el-1-%s-%s", KurtosisConfig.Participants[0].ELType, KurtosisConfig.Participants[0].CLType)
	consensusService = fmt.Sprintf("cl-1-%s-%s", KurtosisConfig.Participants[0].CLType, KurtosisConfig.Participants[0].ELType)
)

// getKurtosisPreset returns the Kurtosis preset to use.
// It retrieves the preset from the environment variable or falls back to the default.
func getKurtosisPreset() string {
	preset := os.Getenv(testvalues.EnvKeyEthereumPosNetworkPreset)
	if preset == "" {
		return testvalues.EnvValueEthereumPosPreset_Minimal
	}
	return preset
}

type EthKurtosisChain struct {
	RPC             string
	BeaconApiClient ethereum.BeaconAPIClient
	Faucet          *ecdsa.PrivateKey

	kurtosisCtx *kurtosis_context.KurtosisContext
	enclaveCtx  *enclaves.EnclaveContext
}

func spinUpKurtosisEnclave(ctx context.Context, enclaveName string, packageId string, packageParams any) (KurtosisEnclave, error) {
	kurtosisCtx, err := kurtosis_context.NewKurtosisContextFromLocalEngine()
	if err != nil {
		return KurtosisEnclave{}, err
	}

	// Check if the enclave already exists and destroy it if it does
	existingEnclaves, err := kurtosisCtx.GetEnclaves(ctx)
	if err != nil {
		return KurtosisEnclave{}, err
	}
	if enclaveInfos, found := existingEnclaves.GetEnclavesByName()[enclaveName]; found {
		for _, enclaveInfo := range enclaveInfos {
			err = kurtosisCtx.DestroyEnclave(ctx, enclaveInfo.EnclaveUuid)
			if err != nil {
				return KurtosisEnclave{}, err
			}
		}
	}
	enclaveCtx, err := kurtosisCtx.CreateEnclave(ctx, enclaveName)
	if err != nil {
		return KurtosisEnclave{}, err
	}

	packageParamJSON, err := json.Marshal(packageParams)
	if err != nil {
		return KurtosisEnclave{}, err
	}
	starlarkResp, err := enclaveCtx.RunStarlarkRemotePackageBlocking(ctx, packageId, &starlark_run_config.StarlarkRunConfig{
		SerializedParams: string(packageParamJSON),
	})
	if err != nil {
		return KurtosisEnclave{}, err
	}
	fmt.Println(starlarkResp.RunOutput)

	return KurtosisEnclave{
		kurtosisCtx: kurtosisCtx,
		enclaveCtx:  enclaveCtx,
	}, nil
}

func (e KurtosisEnclave) Destroy(ctx context.Context) {
	if err := e.kurtosisCtx.DestroyEnclave(ctx, string(e.enclaveCtx.GetEnclaveUuid())); err != nil {
		panic(err)
	}
}

func (e KurtosisEnclave) DumpLogs(ctx context.Context, serviceName string) error {
	enclaveServices, err := e.enclaveCtx.GetServices()
	if err != nil {
		return err
	}

	userServices := make(map[services.ServiceUUID]bool)
	serviceIdToName := make(map[services.ServiceUUID]string)
	for serviceName, servicesUUID := range enclaveServices {
		userServices[servicesUUID] = true
		serviceIdToName[servicesUUID] = string(serviceName)

	}

	stream, cancelFunc, err := e.kurtosisCtx.GetServiceLogs(ctx, string(e.enclaveCtx.GetEnclaveUuid()), userServices, false, true, 0, nil)
	if err != nil {
		return err
	}

	// Dump the stream chan into stdout
	fmt.Println("Dumping kurtosis logs")
	for {
		select {
		case logs, ok := <-stream:
			if !ok {
				return nil
			}
			for serviceID, serviceLog := range logs.GetServiceLogsByServiceUuids() {
				if serviceIdToName[serviceID] != serviceName {
					continue
				}
				for _, log := range serviceLog {
					fmt.Printf("Service %s logs: %s\n", serviceIdToName[serviceID], log)
				}
			}
		case <-ctx.Done():
			cancelFunc()
			return nil
		}
	}
}
