package proofapi

import (
	"testing"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
)

func TestEthToCosmosSelectsV130ModuleForV130WasmTag(t *testing.T) {
	t.Setenv(testvalues.EnvKeyE2EWasmLightClientTag, testvalues.EnvValueWasmLightClientTagV1_3_0)

	config := NewConfigBuilder().EthToCosmos(EthToCosmosParams{}).Build()

	if got := config.Modules[0].Name; got != ModuleEthToCosmosV1_3_0 {
		t.Fatalf("expected module %q, got %q", ModuleEthToCosmosV1_3_0, got)
	}
}

func TestEthToCosmosSelectsMainModuleByDefault(t *testing.T) {
	for _, wasmTag := range []string{"", testvalues.EnvValueWasmLightClientTag_Local, "cw-ics08-wasm-eth-v1.4.0"} {
		t.Run(wasmTag, func(t *testing.T) {
			t.Setenv(testvalues.EnvKeyE2EWasmLightClientTag, wasmTag)

			config := NewConfigBuilder().EthToCosmos(EthToCosmosParams{}).Build()

			if got := config.Modules[0].Name; got != ModuleEthToCosmos {
				t.Fatalf("expected module %q, got %q", ModuleEthToCosmos, got)
			}
		})
	}
}
