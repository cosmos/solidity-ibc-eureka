package chainconfig

import (
	"os"

	interchaintest "github.com/cosmos/interchaintest/v11"
	"github.com/cosmos/interchaintest/v11/ibc"
)

const defaultIbcGoDockerTag = "v11.0.0"

var DefaultChainSpecs = []*interchaintest.ChainSpec{
	// -- IBC-Go --
	IbcGoChainSpec("ibc-go-simd-1", "simd-1"),
}

func IbcGoChainSpec(name, chainId string) *interchaintest.ChainSpec {
	ibcGoDockerTag := os.Getenv("E2E_IBC_GO_DOCKER_TAG")
	if ibcGoDockerTag == "" {
		ibcGoDockerTag = defaultIbcGoDockerTag
	}

	return &interchaintest.ChainSpec{
		ChainConfig: ibc.ChainConfig{
			Type:    "cosmos",
			Name:    name,
			ChainID: chainId,
			Images: []ibc.DockerImage{
				{
					Repository: "ghcr.io/cosmos/ibc-go-wasm-simd", // FOR LOCAL IMAGE USE: Docker Image Name
					Version:    ibcGoDockerTag,                    // FOR LOCAL IMAGE USE: Docker Image Tag
					UIDGID:     "1025:1025",
				},
			},
			Bin:            "simd",
			Bech32Prefix:   "cosmos",
			Denom:          "stake",
			GasPrices:      "0.00stake",
			GasAdjustment:  1.3,
			EncodingConfig: SDKEncodingConfig(),
			ModifyGenesis:  defaultModifyGenesis(),
			TrustingPeriod: "508h",
			NoHostMount:    false,
		},
	}
}

func WfchainChainSpec(name, chainId string) *interchaintest.ChainSpec {
	// sandbox-ledger runs the cosmos-sdk enterprise PoA module, which requires
	// genesis to contain at least one validator with positive power. There is no
	// genutil/gentx integration (validators are not derived from gentxs), so the
	// standard interchaintest bootstrap fails: `gentx` validates the whole genesis,
	// finds zero PoA power and aborts. We therefore skip gentx, run a single
	// validator, and seed the PoA validator set ourselves — discovering the node's
	// CometBFT consensus key in PreGenesis and injecting it in ModifyGenesis.
	numValidators := 1
	numFullNodes := 0
	poaVal := &poaGenesisValidator{}

	return &interchaintest.ChainSpec{
		NumValidators: &numValidators,
		NumFullNodes:  &numFullNodes,
		ChainConfig: ibc.ChainConfig{
			Type:    "cosmos",
			Name:    name,
			ChainID: chainId,
			Images: []ibc.DockerImage{
				{
					Repository: "ghcr.io/cosmos/sandbox-ledger",
					Version:    "v0.0.2",
					// The sandbox-ledger image runs as user `sandbox` (uid/gid 1000).
					// interchaintest chowns the chain's volume to this UIDGID and runs
					// every node command as the image's default user, so a mismatch here
					// makes `init` fail with "couldn't get client config: Config File
					// \"client\" Not Found".
					UIDGID: "1000:1000",
				},
			},
			Bin:            "sandboxd",
			Bech32Prefix:   "cosmos",
			Denom:          "stake",
			GasPrices:      "0.00stake",
			GasAdjustment:  1.5,
			EncodingConfig: SDKEncodingConfig(),
			SkipGenTx:      true,
			PreGenesis:     wfchainPreGenesis(poaVal),
			ModifyGenesis:  wfchainModifyGenesis(poaVal),
			TrustingPeriod: "508h",
			NoHostMount:    false,
		},
	}
}
