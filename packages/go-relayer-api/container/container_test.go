package container_test

import (
	"context"
	"testing"

	"github.com/cosmos/solidity-ibc-eureka/packages/go-relayer-api/config"
	"github.com/cosmos/solidity-ibc-eureka/packages/go-relayer-api/container"
	"github.com/cosmos/solidity-ibc-eureka/packages/go-relayer-api/dockerutil"
	"github.com/stretchr/testify/require"
	"go.uber.org/zap"
)

func TestSpinUpRelayerApiContainer(t *testing.T) {
	docker, err := dockerutil.DockerSetup(t.Name())
	require.NoError(t, err, "failed to set up docker client and network")
	// t.Cleanup(func() {
	// 	if err := docker.Cleanup(); err != nil {
	// 		t.Logf("failed to clean up docker resources: %v", err)
	// 	}
	// })

	relayer, err := container.SpinUpRelayerApiContainer(context.Background(), zap.NewNop(), docker, "v0.5.0", config.NewConfig(config.CreateCosmosCosmosModules(config.CosmosToCosmosConfigInfo{
		ChainAID:    "cosmos-1",
		ChainBID:    "cosmos-2",
		ChainATmRPC: "http://localhost:26657",
		ChainBTmRPC: "http://localhost:26657",
		ChainAUser:  "cosmos-1",
		ChainBUser:  "cosmos-2",
	})), []string{"v1.2.0"})
	require.NoError(t, err)

	err = relayer.Kill()
	require.NoError(t, err)
}
