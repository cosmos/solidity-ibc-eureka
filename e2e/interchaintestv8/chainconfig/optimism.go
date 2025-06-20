package chainconfig

import (
	"context"
	"crypto/ecdsa"
	"fmt"

	ethcommon "github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/crypto"
)

// In case we need it later:
// {
//   "12345": {
//     "proposerPrivateKey": "0x8f407321d6638b35d892c7aed71802a950a57568f27f562ea1dcf2c6a1824de9",
//     "proposerAddress": "0x86dfAFE0689e20685f7872E0cB264868454627Bc",
//     "batcherPrivateKey": "0x5445f4f70eebacf4d593af7e63b3d445b54e0942f4239024fc4f47739a65e0ed",
//     "batcherAddress": "0x0E9c62712ab826E06b16B2236ce542f711EafFaF",
//     "sequencerPrivateKey": "0x8dfe99ac1c99fcde8877a93533012ae66b954d893940deff8c4e9afe25df19ee",
//     "sequencerAddress": "0xbb19dCE4cE51f353A98DBaB31b5fa3bC80DC7769",
//     "challengerPrivateKey": "0xa9f4b570da68ece712f72ffd7ea1f1c5addb5b20ab7dc6adc59a81dd54e57585",
//     "challengerAddress": "0xf1658da627Dd0738C555F9572F658617511C49d5",
//     "l2ProxyAdminPrivateKey": "eaba42282ad33c8ef2524f07277c03a776d98ae19f581990ce75becb7cfa1c23",
//     "l2ProxyAdminAddress": "0x589A698b7b7dA0Bec545177D3963A2741105C7C9",
//     "l1ProxyAdminPrivateKey": "eaba42282ad33c8ef2524f07277c03a776d98ae19f581990ce75becb7cfa1c23",
//     "l1ProxyAdminAddress": "0x589A698b7b7dA0Bec545177D3963A2741105C7C9",
//     "baseFeeVaultRecipientPrivateKey": "eaba42282ad33c8ef2524f07277c03a776d98ae19f581990ce75becb7cfa1c23",
//     "baseFeeVaultRecipientAddress": "0x589A698b7b7dA0Bec545177D3963A2741105C7C9",
//     "l1FeeVaultRecipientPrivateKey": "eaba42282ad33c8ef2524f07277c03a776d98ae19f581990ce75becb7cfa1c23",
//     "l1FeeVaultRecipientAddress": "0x589A698b7b7dA0Bec545177D3963A2741105C7C9",
//     "sequencerFeeVaultRecipientPrivateKey": "eaba42282ad33c8ef2524f07277c03a776d98ae19f581990ce75becb7cfa1c23",
//     "sequencerFeeVaultRecipientAddress": "0x589A698b7b7dA0Bec545177D3963A2741105C7C9",
//     "systemConfigOwnerPrivateKey": "eaba42282ad33c8ef2524f07277c03a776d98ae19f581990ce75becb7cfa1c23",
//     "systemConfigOwnerAddress": "0x589A698b7b7dA0Bec545177D3963A2741105C7C9",
//     "l1FaucetPrivateKey": "0x04b9f63ecf84210c5366c66d68fa1f5da1fa4f634fad6dfc86178e4d79ff9e59",
//     "l1FaucetAddress": "0xafF0CA253b97e54440965855cec0A8a2E2399896",
//     "l2FaucetPrivateKey": "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80",
//     "l2FaucetAddress": "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"
//   }
// }

const (
	kurtosisOptimismPackageId = "github.com/ethpandaops/optimism-package@1.3.0"
	optimismFaucetPrivateKey  = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"

	optimismChainName = "chain0"
	optimismNodeName  = "node0"
)

type KurtosisOptimismChain struct {
	ExecutionRPC string
	ConsensusRPC string
	Faucet       *ecdsa.PrivateKey

	executionService string
	kurtosisEnclave  KurtosisEnclave
}

type kurtosisOptimismConfig struct {
	OptimismPackage kurtosisOptimismPackage `json:"optimism_package"`
}

// To see all the configuration options: github.com/ethpandaops/ethereum-package
type kurtosisOptimismPackage struct {
	Chains map[string]kurtosisOptimismChain `json:"chains"`
}

type kurtosisOptimismChain struct {
	Participants  map[string]kurtosisOptimismParticipant `json:"participants"`
	NetworkParams kustosisOptimismNetworkParams          `json:"network_params,omitempty"`
}

type kurtosisOptimismParticipant struct {
	EL *kurtosisOptimismEL `json:"el,omitempty"`
	CL *kurtosisOptimismCL `json:"cl,omitempty"`
}

type kurtosisOptimismEL struct {
	Type string `json:"type"`
}

type kurtosisOptimismCL struct {
	Type         string            `json:"type"`
	ExtraEnvVars map[string]string `json:"extra_env_vars,omitempty"`
}

type kustosisOptimismNetworkParams struct {
	NetworkID uint64 `json:"network_id"`
}

func SpinUpKurtosisOptimism(ctx context.Context) (KurtosisOptimismChain, error) {
	optimismConfig := kurtosisOptimismConfig{
		OptimismPackage: kurtosisOptimismPackage{
			Chains: map[string]kurtosisOptimismChain{
				optimismChainName: {
					Participants: map[string]kurtosisOptimismParticipant{
						optimismNodeName: {
							EL: &kurtosisOptimismEL{
								Type: "op-geth",
							},
							CL: &kurtosisOptimismCL{
								Type: "op-node",
								ExtraEnvVars: map[string]string{
									"OP_NODE_CONDUCTOR_ENABLED": "true",
								},
							},
						},
					},
					NetworkParams: kustosisOptimismNetworkParams{
						NetworkID: 12345,
					},
				},
			},
		},
	}

	// Extract configuration from the new structure

	chain := optimismConfig.OptimismPackage.Chains[optimismChainName]
	participant := chain.Participants[optimismNodeName]
	elType := participant.EL.Type
	clType := participant.CL.Type
	networkID := chain.NetworkParams.NetworkID

	// Use default values for service naming since the new format doesn't include network params
	executionService := fmt.Sprintf("op-el-%d-%s-%s", networkID, optimismNodeName, elType)
	consensusService := fmt.Sprintf("op-cl-%d-%s-%s", networkID, optimismNodeName, clType)

	kurtosisEnclave, err := spinUpKurtosisEnclave(ctx, "optimism-pos-testnet", kurtosisOptimismPackageId, optimismConfig)
	if err != nil {
		return KurtosisOptimismChain{}, err
	}

	// exeuctionCtx is the service context (kurtosis concept) for the execution node that allows us to get the public ports
	executionCtx, err := kurtosisEnclave.enclaveCtx.GetServiceContext(executionService)
	if err != nil {
		return KurtosisOptimismChain{}, err
	}
	executionRPCPortSpec := executionCtx.GetPublicPorts()["rpc"]
	executionRPC := fmt.Sprintf("http://localhost:%d", executionRPCPortSpec.GetNumber())

	// consensusCtx is the service context for the consensus node (op-node)
	consensusCtx, err := kurtosisEnclave.enclaveCtx.GetServiceContext(consensusService)
	if err != nil {
		return KurtosisOptimismChain{}, err
	}
	consensusRPCPortSpec := consensusCtx.GetPublicPorts()["rpc"]
	consensusRPC := fmt.Sprintf("http://localhost:%d", consensusRPCPortSpec.GetNumber())

	faucet, err := crypto.ToECDSA(ethcommon.FromHex(optimismFaucetPrivateKey))
	if err != nil {
		return KurtosisOptimismChain{}, err
	}

	return KurtosisOptimismChain{
		ExecutionRPC:     executionRPC,
		ConsensusRPC:     consensusRPC,
		Faucet:           faucet,
		executionService: executionService,
		kurtosisEnclave:  kurtosisEnclave,
	}, nil // Implement the logic to spin up the chain
}

func (e KurtosisOptimismChain) Destroy(ctx context.Context) {
	e.kurtosisEnclave.Destroy(ctx)
}

func (e KurtosisOptimismChain) DumpLogs(ctx context.Context) error {
	return e.kurtosisEnclave.DumpLogs(ctx, e.executionService)
}
