package container_test

import (
	"context"
	"testing"

	"github.com/cosmos/solidity-ibc-eureka/packages/go-relayer-api/config"
	"github.com/cosmos/solidity-ibc-eureka/packages/go-relayer-api/container"
	"github.com/cosmos/solidity-ibc-eureka/packages/go-relayer-api/dockerutil"
	"github.com/stretchr/testify/require"
	"go.uber.org/zap"
	"go.uber.org/zap/zaptest"
)

func TestSpinUpRelayerApiContainer(t *testing.T) {
	ctx := context.Background()
	logger := zaptest.NewLogger(t)
	docker, err := dockerutil.DockerSetup(ctx, logger, t.Name())
	require.NoError(t, err, "failed to set up docker client and network")

	// Verify that there are no remaining containers or volumes
	containers, err := docker.Containers(ctx)
	require.NoError(t, err, "failed to list containers")
	require.Empty(t, containers, "expected no containers to be running")

	t.Cleanup(func() {
		ctx := context.Background()
		if err := docker.Cleanup(ctx, true); err != nil {
			t.Logf("failed to clean up docker resources: %v", err)
		}
	})

	relayer, err := container.SpinUpRelayerApiContainer(context.Background(), zap.NewNop(), docker, "v0.5.0", config.NewConfig(config.CreateCosmosCosmosModules(config.CosmosToCosmosConfigInfo{
		ChainAID:    "cosmos-1",
		ChainBID:    "cosmos-2",
		ChainATmRPC: "http://localhost:26657",
		ChainBTmRPC: "http://localhost:26657",
		ChainAUser:  "cosmos-1",
		ChainBUser:  "cosmos-2",
	})), []string{"v1.2.0"})
	require.NoError(t, err)
	require.NotNil(t, relayer)
}
