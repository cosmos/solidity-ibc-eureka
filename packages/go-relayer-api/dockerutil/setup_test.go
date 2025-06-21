package dockerutil_test

import (
	"context"
	"fmt"
	"os"
	"testing"

	volumetypes "github.com/docker/docker/api/types/volume"
	"github.com/moby/moby/errdefs"
	"github.com/stretchr/testify/require"

	"github.com/cosmos/solidity-ibc-eureka/packages/go-relayer-api/dockerutil"
)

func TestDockerSetup_KeepVolumes(t *testing.T) {
	if testing.Short() {
		t.Skip("skipping due to short mode")
	}

	docker, err := dockerutil.DockerSetup(t.Name())
	require.NoError(t, err, "failed to set up docker client and network")
	t.Cleanup(func() {
		if err := docker.Cleanup(); err != nil {
			t.Logf("failed to clean up docker resources: %v", err)
		}
	})

	ctx := context.Background()

	for _, tc := range []struct {
		keep bool
	}{
		{keep: false},
		{keep: true},
	} {
		testName := fmt.Sprintf("keep=%t", tc.keep)
		t.Run(testName, func(t *testing.T) {
			os.Setenv(dockerutil.KeepContainersEnvKey, fmt.Sprintf("%t", tc.keep))

			var volumeName string
			cli, err := dockerutil.DockerSetup(testName)
			require.NoError(t, err, "failed to set up docker client and network")

			v, err := cli.Client.VolumeCreate(ctx, volumetypes.CreateOptions{
				Labels: map[string]string{dockerutil.RunLabel: testName},
			})
			require.NoError(t, err)

			volumeName = v.Name

			_, err = cli.Client.VolumeInspect(ctx, volumeName)
			if !tc.keep {
				require.Truef(t, errdefs.IsNotFound(err), "expected not found error, got %v", err)
				return
			}

			require.NoError(t, err)
			if err := cli.Client.VolumeRemove(ctx, volumeName, true); err != nil {
				t.Logf("failed to remove volume %s: %v", volumeName, err)
			}
		})
	}
}
