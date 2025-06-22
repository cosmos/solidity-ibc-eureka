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
	"github.com/docker/docker/api/types/container"
	"github.com/docker/docker/api/types/filters"
	"github.com/docker/docker/api/types/network"
	"github.com/moby/moby/client"
	"github.com/moby/moby/errdefs"
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
}

func DockerSetup(runLabelValue string) (Docker, error) {
	cli, err := client.NewClientWithOpts(client.FromEnv)
	if err != nil {
		return Docker{}, fmt.Errorf("failed to create docker client: %v", err)
	}

	// Also eagerly clean up any leftover resources from a previous test run,
	// e.g. if the test was interrupted.
	if err := dockerCleanupFn(cli, runLabelValue)(); err != nil {
		return Docker{}, fmt.Errorf("failed to clean up docker resources: %w", err)
	}

	name := fmt.Sprintf("%s-%s", DockerPrefix, RandLowerCaseLetterString(8))
	octet := uint8(rand.Intn(256))
	baseSubnet := fmt.Sprintf("172.%d.0.0/16", octet)
	usedSubnets, err := getUsedSubnets(cli)
	if err != nil {
		return Docker{}, fmt.Errorf("failed to get used subnets: %v", err)
	}
	subnet, err := findAvailableSubnet(baseSubnet, usedSubnets)
	if err != nil {
		return Docker{}, fmt.Errorf("failed to find available subnet: %v", err)
	}
	network, err := cli.NetworkCreate(context.TODO(), name, network.CreateOptions{
		Driver: "bridge",
		IPAM: &network.IPAM{
			Config: []network.IPAMConfig{
				{
					Subnet: subnet,
				},
			},
		},

		Labels: map[string]string{RunLabel: runLabelValue},
	})
	if err != nil {
		panic(fmt.Errorf("failed to create docker network: %v", err))
	}

	return Docker{
		RunLabelValue: runLabelValue,
		Client:        cli,
		NetworkID:     network.ID,
	}, nil
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
func (d Docker) Cleanup() error {
	return dockerCleanupFn(d.Client, d.RunLabelValue)()
}

func dockerCleanupFn(cli *client.Client, runLabelValue string) func() error {
	return func() error {
		showContainerLogs := os.Getenv(ShowContainerLogsEnvKey)
		containerLogTail := os.Getenv(ContainerLogTailEnvKey)
		keepContainers := os.Getenv(KeepContainersEnvKey) != ""

		ctx := context.TODO()
		cli.NegotiateAPIVersion(ctx)
		cs, err := cli.ContainerList(ctx, container.ListOptions{
			All: true,
			Filters: filters.NewArgs(
				filters.Arg("label", fmt.Sprintf("%s=%s", RunLabel, runLabelValue)),
			),
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
				rc, err := cli.ContainerLogs(ctx, c.ID, container.LogsOptions{
					ShowStdout: true,
					ShowStderr: true,
					Tail:       logTail,
				})
				if err == nil {
					b := new(bytes.Buffer)
					_, err := b.ReadFrom(rc)
					if err == nil {
						fmt.Printf("Logs for container %s:\n%s\n", c.ID, b.String())
					}
				}
			}
			if !keepContainers {
				var stopTimeout container.StopOptions
				timeout := 10
				timeoutDur := time.Duration(timeout * int(time.Second))
				deadline := time.Now().Add(timeoutDur)
				stopTimeout.Timeout = &timeout
				if err := cli.ContainerStop(ctx, c.ID, stopTimeout); IsLoggableStopError(err) {
					return err
				}

				waitCtx, cancel := context.WithDeadline(ctx, deadline.Add(500*time.Millisecond))
				waitCh, errCh := cli.ContainerWait(waitCtx, c.ID, container.WaitConditionNotRunning)
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

				if err := cli.ContainerRemove(ctx, c.ID, container.RemoveOptions{
					// Not removing volumes with the container, because we separately handle them conditionally.
					Force: true,
				}); err != nil {
					return fmt.Errorf("failed to remove container %s: %w", c.ID, err)
				}
			}
		}

		if !keepContainers {
			if err := PruneVolumesWithRetry(ctx, cli, runLabelValue); err != nil {
				return err
			}
			if err := PruneNetworksWithRetry(ctx, cli, runLabelValue); err != nil {
				return err
			}
		}

		return nil
	}
}

func PruneVolumesWithRetry(ctx context.Context, cli *client.Client, runLabelValue string) error {
	var msg string
	err := retry.Do(
		func() error {
			res, err := cli.VolumesPrune(ctx, filters.NewArgs(filters.Arg("label", RunLabel+"="+runLabelValue)))
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
	// if msg != "" {
	// 	// Odd to Logf %s, but this is a defensive way to keep the DockerSetupTestingT interface
	// 	// with only Logf and not need to add Log.
	// 	t.Logf("%s", msg)
	// }

	return nil
}

func PruneNetworksWithRetry(ctx context.Context, cli *client.Client, runLabelValue string) error {
	var deleted []string
	err := retry.Do(
		func() error {
			res, err := cli.NetworksPrune(ctx, filters.NewArgs(filters.Arg("label", RunLabel+"="+runLabelValue)))
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

func IsLoggableStopError(err error) bool {
	if err == nil {
		return false
	}
	return !(errdefs.IsNotModified(err) || errdefs.IsNotFound(err))
}
