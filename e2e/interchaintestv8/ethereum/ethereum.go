package ethereum

import (
	"bytes"
	"context"
	"crypto/ecdsa"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"math/big"
	"os"
	"os/exec"
	"strconv"
	"strings"
	"time"

	"cosmossdk.io/math"
	ethcommon "github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/crypto"
	"github.com/ethereum/go-ethereum/ethclient"
	"github.com/kurtosis-tech/kurtosis/api/golang/core/lib/enclaves"
	"github.com/kurtosis-tech/kurtosis/api/golang/core/lib/services"
	"github.com/kurtosis-tech/kurtosis/api/golang/core/lib/starlark_run_config"
	"github.com/kurtosis-tech/kurtosis/api/golang/engine/lib/kurtosis_context"
	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
)

type Ethereum struct {
	kurtosisCtx *kurtosis_context.KurtosisContext
	enclaveCtx  *enclaves.EnclaveContext

	ChainID         *big.Int
	RPC             string
	EthAPI          EthAPI
	BeaconAPIClient BeaconAPIClient

	Faucet *ecdsa.PrivateKey
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

func SpinUpEthereum(ctx context.Context) (Ethereum, error) {
	// get faucet private key from string
	faucet, err := crypto.ToECDSA(ethcommon.FromHex(testvalues.FaucetPrivateKey))
	if err != nil {
		return Ethereum{}, err
	}

	kurtosisCtx, err := kurtosis_context.NewKurtosisContextFromLocalEngine()
	if err != nil {
		return Ethereum{}, err
	}

	enclaveName := "ethereum-pos-testnet"
	enclaves, err := kurtosisCtx.GetEnclaves(ctx)
	if err != nil {
		return Ethereum{}, err
	}

	if enclaveInfos, found := enclaves.GetEnclavesByName()[enclaveName]; found {
		for _, enclaveInfo := range enclaveInfos {
			err = kurtosisCtx.DestroyEnclave(ctx, enclaveInfo.EnclaveUuid)
			if err != nil {
				return Ethereum{}, err
			}
		}
	}
	enclaveCtx, err := kurtosisCtx.CreateEnclave(ctx, enclaveName)
	if err != nil {
		return Ethereum{}, err
	}
	networkParams := KurtosisNetworkParams{
		Participants: []Participant{
			{
				CLType:        "lighthouse",
				CLImage:       "sigp/lighthouse:latest-unstable",
				CLExtraParams: []string{"--light-client-server"},
				ELImage:       "ethereum/client-go:v1.14.6",
				ELLogLevel:    "info",
			},
		},
	}
	networkParamsJson, err := json.Marshal(networkParams)
	if err != nil {
		return Ethereum{}, err
	}
	starlarkResp, err := enclaveCtx.RunStarlarkRemotePackageBlocking(ctx, "github.com/ethpandaops/ethereum-package", &starlark_run_config.StarlarkRunConfig{
		SerializedParams: string(networkParamsJson),
	})
	if err != nil {
		return Ethereum{}, err
	}
	fmt.Println(starlarkResp.RunOutput)

	time.Sleep(45 * time.Second)

	gethCtx, err := enclaveCtx.GetServiceContext("el-1-geth-lighthouse")
	if err != nil {
		return Ethereum{}, err
	}
	rpcPortSpec := gethCtx.GetPublicPorts()["rpc"]
	rpc := fmt.Sprintf("http://localhost:%d", rpcPortSpec.GetNumber())
	ethClient, err := ethclient.Dial(rpc)
	if err != nil {
		return Ethereum{}, err
	}
	chainID, err := ethClient.ChainID(ctx)
	if err != nil {
		return Ethereum{}, err
	}
	ethAPI, err := NewEthAPI(rpc)
	if err != nil {
		return Ethereum{}, err
	}

	lighthouseCtx, err := enclaveCtx.GetServiceContext("cl-1-lighthouse-geth")
	if err != nil {
		return Ethereum{}, err
	}
	beaconPortSpec := lighthouseCtx.GetPublicPorts()["http"]
	beaconRPC := fmt.Sprintf("http://localhost:%d", beaconPortSpec.GetNumber())

	return Ethereum{
		kurtosisCtx:     kurtosisCtx,
		enclaveCtx:      enclaveCtx,
		ChainID:         chainID,
		RPC:             rpc,
		EthAPI:          ethAPI,
		BeaconAPIClient: NewBeaconAPIClient(beaconRPC),
		Faucet:          faucet,
	}, nil
}

func (e Ethereum) Destroy(ctx context.Context) {
	if err := e.kurtosisCtx.DestroyEnclave(ctx, string(e.enclaveCtx.GetEnclaveUuid())); err != nil {
		panic(err)
	}
}

func (e Ethereum) DumpLogs(ctx context.Context) error {
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
				if serviceIdToName[serviceID] != "el-1-geth-lighthouse" {
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

func (e Ethereum) ForgeScript(deployer *ecdsa.PrivateKey, solidityContract string) ([]byte, error) {
	cmd := exec.Command("forge", "script", "--rpc-url", e.RPC, "--broadcast", "--delay", "30", "--slow", "--non-interactive", "-vvvv", solidityContract)
	cmd.Env = append(cmd.Env, fmt.Sprintf("PRIVATE_KEY=0x%s", hex.EncodeToString(deployer.D.Bytes())))

	var stdoutBuf bytes.Buffer

	// Create a MultiWriter to write to both os.Stdout and the buffer
	multiWriter := io.MultiWriter(os.Stdout, &stdoutBuf)

	// Set the command's stdout to the MultiWriter
	cmd.Stdout = multiWriter
	cmd.Stderr = os.Stderr

	// Run the command
	if err := cmd.Run(); err != nil {
		fmt.Println("Error start command", cmd.Args, err)
		return nil, err
	}

	// Get the output as byte slices
	stdoutBytes := stdoutBuf.Bytes()

	return stdoutBytes, nil
}

func (e Ethereum) CreateAndFundUser() (*ecdsa.PrivateKey, error) {
	key, err := crypto.GenerateKey()
	if err != nil {
		return nil, err
	}

	address := crypto.PubkeyToAddress(key.PublicKey).Hex()
	if err := e.FundUser(address, testvalues.StartingEthBalance); err != nil {
		return nil, err
	}

	return key, nil
}

func (e Ethereum) FundUser(address string, amount math.Int) error {
	return e.SendEth(e.Faucet, address, amount)
}

func (e Ethereum) SendEth(key *ecdsa.PrivateKey, toAddress string, amount math.Int) error {
	cmd := exec.Command(
		"cast",
		"send",
		toAddress,
		"--value", amount.String(),
		"--private-key", fmt.Sprintf("0x%s", ethcommon.Bytes2Hex(key.D.Bytes())),
		"--rpc-url", e.RPC,
	)
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr

	return cmd.Run()
}

func (e *Ethereum) Height() (int64, error) {
	cmd := exec.Command("cast", "block-number", "--rpc-url", e.RPC)
	stdout, err := cmd.Output()
	if err != nil {
		return 0, err
	}
	return strconv.ParseInt(strings.TrimSpace(string(stdout)), 10, 64)
}
