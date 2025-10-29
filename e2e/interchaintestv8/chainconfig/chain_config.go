package chainconfig

import (
	interchaintest "github.com/strangelove-ventures/interchaintest/v8"
	"github.com/strangelove-ventures/interchaintest/v8/ibc"
)

var DefaultChainSpecs = []*interchaintest.ChainSpec{
	// -- IBC-Go --
	IbcGoChainSpec("ibc-go-simd-1", "simd-1"),
}

func IbcGoChainSpec(name, chainId string) *interchaintest.ChainSpec {
	return &interchaintest.ChainSpec{
		ChainConfig: ibc.ChainConfig{
			Type:    "cosmos",
			Name:    name,
			ChainID: chainId,
			Images: []ibc.DockerImage{
				{
<<<<<<< HEAD
					Repository: "ghcr.io/cosmos/ibc-go-wasm-simd", // FOR LOCAL IMAGE USE: Docker Image Name
					Version:    "release-v10.1.x",                 // FOR LOCAL IMAGE USE: Docker Image Tag
					UidGid:     "1025:1025",
=======
					Repository: "ghcr.io/cosmos/ibc-go-wasm-simd",       // FOR LOCAL IMAGE USE: Docker Image Name
					Version:    "modules-light-clients-08-wasm-v10.4.0", // FOR LOCAL IMAGE USE: Docker Image Tag
					UIDGID:     "1025:1025",
>>>>>>> ba92798 (deps: bumped `forge-std`, `kurtosis`, `ibc-go` (#798))
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
