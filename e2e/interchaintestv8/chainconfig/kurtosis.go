package chainconfig

import (
	"context"
	"encoding/json"
	"fmt"

	"github.com/cosmos/interchaintest/v10/testutil"
	"github.com/kurtosis-tech/kurtosis/api/golang/core/lib/enclaves"
	"github.com/kurtosis-tech/kurtosis/api/golang/core/lib/services"
	"github.com/kurtosis-tech/kurtosis/api/golang/core/lib/starlark_run_config"
	"github.com/kurtosis-tech/kurtosis/api/golang/engine/lib/kurtosis_context"
	// ethcommon "github.com/ethereum/go-ethereum/common"
	// "github.com/ethereum/go-ethereum/crypto"
	//
	// "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/ethereum"
	// "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
)

type KurtosisEnclave struct {
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
