package dockerutil

import (
	"bytes"
	"context"
	"fmt"
	"math/rand"
	"net"
	"os"
	"time"

	"github.com/avast/retry-go/v4"
	dockertypes "github.com/docker/docker/api/types"
	"github.com/docker/docker/api/types/container"
	"github.com/docker/docker/api/types/filters"
	"github.com/docker/docker/api/types/network"
	"github.com/moby/moby/client"
	"github.com/moby/moby/errdefs"
	"go.uber.org/zap"
)

const (
	ShowContainerLogsEnvKey = "SHOW_CONTAINER_LOGS"
	ContainerLogTailEnvKey  = "CONTAINER_LOG_TAIL"
	KeepContainersEnvKey    = "KEEP_CONTAINERS"

	// LabelPrefix is the reverse DNS format "namespace" for interchaintest Docker labels.
	LabelPrefix = "com.cosmos.ibc."

	// NodeOwnerLabel indicates the logical node owning a particular object (probably a volume).
	NodeOwnerLabel = LabelPrefix + "node-owner"

	RunLabel = LabelPrefix + "run"
)

// TODO: Rename
type Docker struct {
	RunLabelValue string
	Client        *client.Client
	NetworkID     string

	logger *zap.Logger
}

func newDockerWithCleanup(ctx context.Context, logger *zap.Logger, runLabelValue string, client *client.Client, networkID string) (Docker, error) {
	d := Docker{
		RunLabelValue: SanitizeLabelValue(runLabelValue),
		Client:        client,
		NetworkID:     networkID,
		logger:        logger,
	}
	if err := d.Cleanup(ctx, true); err != nil {
		return Docker{}, fmt.Errorf("failed to clean up docker resources: %w", err)
	}
	return d, nil
}

func DockerWithExistingSetup(ctx context.Context, logger *zap.Logger, runLabelValue string, client *client.Client, networkID string) (Docker, error) {
	return newDockerWithCleanup(ctx, logger, runLabelValue, client, networkID)
}

func DockerSetup(ctx context.Context, logger *zap.Logger, runLabelValue string) (Docker, error) {
	c, err := client.NewClientWithOpts(client.FromEnv)
	if err != nil {
		return Docker{}, fmt.Errorf("failed to create docker client: %v", err)
	}
	d, err := newDockerWithCleanup(ctx, logger, runLabelValue, c, "")

	name := fmt.Sprintf("%s-%s", DockerPrefix, RandLowerCaseLetterString(8))
	octet := uint8(rand.Intn(256))
	baseSubnet := fmt.Sprintf("172.%d.0.0/16", octet)
	usedSubnets, err := getUsedSubnets(d.Client)
	if err != nil {
		return Docker{}, fmt.Errorf("failed to get used subnets: %v", err)
	}
	subnet, err := findAvailableSubnet(baseSubnet, usedSubnets)
	if err != nil {
		return Docker{}, fmt.Errorf("failed to find available subnet: %v", err)
	}
	network, err := c.NetworkCreate(context.TODO(), name, network.CreateOptions{
		Driver: "bridge",
		IPAM: &network.IPAM{
			Config: []network.IPAMConfig{
				{
					Subnet: subnet,
				},
			},
		},

		Labels: map[string]string{RunLabel: d.RunLabelValue},
	})
	if err != nil {
		panic(fmt.Errorf("failed to create docker network: %v", err))
	}
	d.NetworkID = network.ID

	return d, nil
}

func (d *Docker) RunLabelFilter() filters.Args {
	return filters.NewArgs(filters.Arg("label", fmt.Sprintf("%s=%s", RunLabel, d.RunLabelValue)))
}

func getUsedSubnets(cli *client.Client) (map[string]bool, error) {
	usedSubnets := make(map[string]bool)
	networks, err := cli.NetworkList(context.TODO(), network.ListOptions{})
	if err != nil {
		return nil, err
	}

	for _, net := range networks {
		for _, config := range net.IPAM.Config {
			if config.Subnet != "" {
				usedSubnets[config.Subnet] = true
			}
		}
	}
	return usedSubnets, nil
}

func findAvailableSubnet(baseSubnet string, usedSubnets map[string]bool) (string, error) {
	ip, ipNet, err := net.ParseCIDR(baseSubnet)
	if err != nil {
		return "", fmt.Errorf("invalid base subnet: %v", err)
	}

	for {
		if isSubnetUsed(ipNet.String(), usedSubnets) {
			incrementIP(ip, 2)
			ipNet.IP = ip
			continue
		}

		for subIP := ip.Mask(ipNet.Mask); ipNet.Contains(subIP); incrementIP(subIP, 1) {
			subnet := fmt.Sprintf("%s/24", subIP)

			if !isSubnetUsed(subnet, usedSubnets) {
				return subnet, nil
			}
		}

		incrementIP(ip, 2)
		ipNet.IP = ip
	}
}

func isSubnetUsed(subnet string, usedSubnets map[string]bool) bool {
	_, targetNet, err := net.ParseCIDR(subnet)
	if err != nil {
		return true
	}

	for usedSubnet := range usedSubnets {
		_, usedNet, err := net.ParseCIDR(usedSubnet)
		if err != nil {
			continue
		}

		if usedNet.Contains(targetNet.IP) || targetNet.Contains(usedNet.IP) {
			return true
		}
	}
	return false
}

func incrementIP(ip net.IP, incrementLevel int) {
	for j := len(ip) - incrementLevel; j >= 0; j-- {
		ip[j]++
		if ip[j] > 0 {
			break
		}
	}
}

// DockerCleanup will clean up Docker containers, networks, and the other various config files generated in testing.
func (d *Docker) Cleanup(ctx context.Context, force bool) error {
	showContainerLogs := os.Getenv(ShowContainerLogsEnvKey)
	containerLogTail := os.Getenv(ContainerLogTailEnvKey)
	keepContainers := os.Getenv(KeepContainersEnvKey) == "true"
	if force {
		keepContainers = false
	}

	d.logger.Info("Docker cleanup with options", zap.String("showContainerLogs", showContainerLogs), zap.String("containerLogTail", containerLogTail), zap.Bool("keepContainers", keepContainers))

	d.Client.NegotiateAPIVersion(ctx)
	cs, err := d.Client.ContainerList(ctx, container.ListOptions{
		All:     true,
		Filters: d.RunLabelFilter(),
	})
	if err != nil {
		return fmt.Errorf("failed to list containers for cleanup: %w", err)
	}

	for _, c := range cs {
		if showContainerLogs == "" || showContainerLogs == "always" {
			logTail := "50"
			if containerLogTail != "" {
				logTail = containerLogTail
			}
			rc, err := d.Client.ContainerLogs(ctx, c.ID, container.LogsOptions{
				ShowStdout: true,
				ShowStderr: true,
				Tail:       logTail,
			})
			if err == nil {
				b := new(bytes.Buffer)
				_, err := b.ReadFrom(rc)
				if err == nil {
					d.logger.Info("Container logs", zap.String("containerID", c.ID), zap.String("logs", b.String()))
				}
			}
		}
		if !keepContainers {
			var stopTimeout container.StopOptions
			timeout := 10
			timeoutDur := time.Duration(timeout * int(time.Second))
			deadline := time.Now().Add(timeoutDur)
			stopTimeout.Timeout = &timeout
			if err := d.Client.ContainerStop(ctx, c.ID, stopTimeout); IsLoggableStopError(err) {
				return err
			}

			waitCtx, cancel := context.WithDeadline(ctx, deadline.Add(500*time.Millisecond))
			waitCh, errCh := d.Client.ContainerWait(waitCtx, c.ID, container.WaitConditionNotRunning)
			select {
			case <-waitCtx.Done():
			case err := <-errCh:
				cancel()
				return fmt.Errorf("error waiting for container %s to stop: %w", c.ID, err)
			case res := <-waitCh:
				cancel()
				if res.Error != nil {
					return fmt.Errorf("error waiting for container %s to stop: %v", c.ID, res.Error)
				}
				// Ignoring statuscode for now.
			}
			cancel()

			if err := d.Client.ContainerRemove(ctx, c.ID, container.RemoveOptions{
				// Not removing volumes with the container, because we separately handle them conditionally.
				Force: true,
			}); err != nil {
				return fmt.Errorf("failed to remove container %s: %w", c.ID, err)
			}
		}
	}

	if !keepContainers {
		if err := d.PruneVolumesWithRetry(ctx); err != nil {
			return err
		}

		if d.NetworkID != "" {
			if err := d.PruneNetworksWithRetry(ctx); err != nil {
				return err
			}
		}
	}

	return nil

}

func (d *Docker) PruneVolumesWithRetry(ctx context.Context) error {
	volumes, _ := d.Volumes(ctx)
	d.logger.Info("Starting pruning volumes", zap.Int("volumes", len(volumes)), zap.String("label", d.RunLabelValue))
	var msg string
	err := retry.Do(
		func() error {
			res, err := d.Client.VolumesPrune(ctx, d.RunLabelFilter())
			if err != nil {
				if errdefs.IsConflict(err) {
					// Prune is already in progress; try again.
					return err
				}

				// Give up on any other error.
				return retry.Unrecoverable(err)
			}

			if len(res.VolumesDeleted) > 0 {
				msg = fmt.Sprintf("Pruned %d volumes, reclaiming approximately %.1f MB", len(res.VolumesDeleted), float64(res.SpaceReclaimed)/(1024*1024))
			}

			return nil
		},
		retry.Context(ctx),
		retry.DelayType(retry.FixedDelay),
	)
	if err != nil {
		return fmt.Errorf("failed to prune volumes: %w", err)
	}

	_ = msg
	if msg != "" {
		d.logger.Info(msg)
	}

	return nil
}

func (d *Docker) PruneNetworksWithRetry(ctx context.Context) error {
	d.logger.Info("Purning networks")
	var deleted []string
	err := retry.Do(
		func() error {
			res, err := d.Client.NetworksPrune(ctx, filters.NewArgs(filters.Arg("label", RunLabel+"="+d.RunLabelValue)))
			if err != nil {
				if errdefs.IsConflict(err) {
					// Prune is already in progress; try again.
					return err
				}

				// Give up on any other error.
				return retry.Unrecoverable(err)
			}

			deleted = res.NetworksDeleted
			return nil
		},
		retry.Context(ctx),
		retry.DelayType(retry.FixedDelay),
	)
	if err != nil {
		return fmt.Errorf("failed to prune networks: %w", err)
	}

	_ = deleted
	// if len(deleted) > 0 {
	// 	// TODO: Log this.
	// 	// fmt.Printf("pruned %d networks: %s", len(deleted), strings.Join(deleted, ", "))
	// }

	return nil
}

func (d *Docker) Containers(ctx context.Context) ([]dockertypes.Container, error) {
	cs, err := d.Client.ContainerList(ctx, container.ListOptions{
		All:     true,
		Filters: d.RunLabelFilter(),
	})
	if err != nil {
		return nil, fmt.Errorf("failed to list containers: %w", err)
	}
	return cs, nil
}

func IsLoggableStopError(err error) bool {
	if err == nil {
		return false
	}
	return !(errdefs.IsNotModified(err) || errdefs.IsNotFound(err))
}
