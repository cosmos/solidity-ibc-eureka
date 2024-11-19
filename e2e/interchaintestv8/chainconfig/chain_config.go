package chainconfig

import (
	interchaintest "github.com/strangelove-ventures/interchaintest/v8"
	"github.com/strangelove-ventures/interchaintest/v8/ibc"
)

var DefaultChainSpecs = []*interchaintest.ChainSpec{
	// -- IBC-Go --
	{
		ChainConfig: ibc.ChainConfig{
			Type:    "cosmos",
			Name:    "ibc-go-simd",
			ChainID: "simd-1",
			Images: []ibc.DockerImage{
				{
					Repository: "ghcr.io/cosmos/ibc-go-wasm-simd",                        // FOR LOCAL IMAGE USE: Docker Image Name
					Version:    "cian-issue-7510-ibcmodulev2-transfer-application-spike", // FOR LOCAL IMAGE USE: Docker Image Tag
					UidGid:     "1025:1025",
				},
			},
			Bin:            "simd",
			Bech32Prefix:   "cosmos",
			Denom:          "stake",
			GasPrices:      "0.00stake",
			GasAdjustment:  1.3,
			EncodingConfig: CosmosEncodingConfig(),
			ModifyGenesis:  defaultModifyGenesis(),
			TrustingPeriod: "508h",
			NoHostMount:    false,
		},
	},
}
