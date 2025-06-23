package dockerutil_test

import (
	"context"
	"fmt"
	"os"
	"testing"

	"github.com/docker/docker/api/types/volume"
	"github.com/docker/go-connections/nat"
	"github.com/moby/moby/errdefs"
	"github.com/stretchr/testify/require"
	"go.uber.org/zap/zaptest"

	"github.com/cosmos/solidity-ibc-eureka/packages/go-relayer-api/dockerutil"
)

func TestDockerSetup_KeepVolumes(t *testing.T) {
	if testing.Short() {
		t.Skip("skipping due to short mode")
	}

	ctx := context.Background()
	logger := zaptest.NewLogger(t)

	for _, tc := range []struct {
		keep bool
	}{
		{keep: false},
		{keep: true},
	} {
		testName := fmt.Sprintf("keep=%t", tc.keep)
		t.Run(testName, func(t *testing.T) {
			err := os.Setenv(dockerutil.KeepContainersEnvKey, fmt.Sprintf("%t", tc.keep))
			require.NoError(t, err)
			val := os.Getenv(dockerutil.KeepContainersEnvKey)
			fmt.Printf("Environment variable %s is set to %s\n", dockerutil.KeepContainersEnvKey, val)

			var volumeName string
			docker, err := dockerutil.DockerSetup(ctx, logger, testName)
			require.NoError(t, err, "failed to set up docker client and network")

			v, err := docker.CreateVolume(ctx)

			volumeName = v.Name

			// verify we have 1 volume
			volumes, err := docker.Volumes(ctx)
			require.NoError(t, err)
			require.Len(t, volumes, 1)

			err = docker.Cleanup(ctx, false)
			require.NoError(t, err)

			volumes, err = docker.Volumes(ctx)
			require.NoError(t, err)

			_, err = docker.Client.VolumeInspect(ctx, volumeName)
			if !tc.keep {
				require.Len(t, volumes, 0)

				require.Truef(t, errdefs.IsNotFound(err), "expected not found error for volume %s, got %v", volumeName, err)
				return
			}

			require.NoError(t, err)
			require.Len(t, volumes, 1)

			if err := docker.Client.VolumeRemove(ctx, volumeName, true); err != nil {
				t.Logf("failed to remove volume %s: %v", volumeName, err)
			}
		})
	}
}

func TestDockerSetup_Cleanup(t *testing.T) {
	if testing.Short() {
		t.Skip("skipping due to short mode")
	}

	ctx := context.Background()
	logger := zaptest.NewLogger(t)

	docker, err := dockerutil.DockerSetup(ctx, logger, t.Name())
	require.NoError(t, err, "failed to set up docker client and network")
	t.Cleanup(func() {
		if err := docker.Cleanup(ctx, true); err != nil {
			t.Logf("failed to clean up docker resources: %v", err)
		}
	})

	image := dockerutil.NewImage(logger, docker.Client, docker.NetworkID, docker.RunLabelValue, "busybox", "latest")
	portMap := nat.PortMap{
		nat.Port("3000/tcp"): {},
	}
	v, err := docker.Client.VolumeCreate(ctx, volume.CreateOptions{
		Labels: map[string]string{dockerutil.RunLabel: docker.RunLabelValue},
	})
	require.NoError(t, err)
	volumeBinds := []string{fmt.Sprintf("%s:%s", v.Name, "/relayer")}
	cmd := []string{"echo", "-n", "hello"}
	env := []string{}
	containerLifecycle := dockerutil.NewContainerLifecycle(logger, docker.Client, "derpycontainername")
	err = containerLifecycle.CreateContainer(ctx, docker.RunLabelValue, docker.NetworkID, image, portMap, volumeBinds, nil, "", cmd, env, []string{})

	// verify that we have a container
	containers, err := docker.Containers(ctx)
	require.NoError(t, err)
	require.Len(t, containers, 1)

	err = docker.Cleanup(ctx, true)
	require.NoError(t, err, "failed to clean up docker resources without keeping containers")

	// verify that we don't have any containers after cleanup
	containers, err = docker.Containers(ctx)
	require.NoError(t, err)
	require.Len(t, containers, 0)
}
