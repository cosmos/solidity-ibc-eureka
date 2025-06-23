package container

import (
	"context"
	"errors"
	"fmt"
	"strings"
	"time"

	"github.com/cosmos/solidity-ibc-eureka/packages/go-relayer-api/config"
	"github.com/cosmos/solidity-ibc-eureka/packages/go-relayer-api/dockerutil"
	"github.com/docker/docker/api/types/volume"
	"github.com/docker/go-connections/nat"
	"go.uber.org/zap"
	grpc "google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"
)

const (
	relayerPort           = "3000/tcp"
	relayerPrometheusPort = "9000/tcp"
)

// TODO: Rename
type RelayerApiContainer struct {
	containerLifecycle   *dockerutil.ContainerLifecycle
	relayerServiceClient RelayerServiceClient
}

func SpinUpRelayerApiContainer(ctx context.Context, log *zap.Logger, docker dockerutil.Docker, tag string, config config.Config, sp1ProgramVersions []string) (RelayerApiContainer, error) {
	containerLifecycle := dockerutil.NewContainerLifecycle(log, docker.Client, fmt.Sprintf("%s-%s", docker.RunLabelValue, "relayer-api"))
	cmd := []string{
		"sh",
		"/docker_entrypoint.sh",
		"start",
		"--config",
		"/relayer/relayer.json",
	}
	image := dockerutil.NewImage(log, docker.Client, docker.NetworkID, docker.RunLabelValue, "ghcr.io/cosmos/eureka-relayer", tag)
	sp1ProgramVersionsStr := strings.Join(sp1ProgramVersions, " ")
	env := []string{
		fmt.Sprintf("SP1_PROGRAM_VERSIONS=%s", sp1ProgramVersionsStr),
	}
	portMap := nat.PortMap{
		nat.Port(relayerPort):           {},
		nat.Port(relayerPrometheusPort): {},
	}
	v, err := docker.Client.VolumeCreate(ctx, volume.CreateOptions{
		Labels: map[string]string{dockerutil.RunLabel: docker.RunLabelValue},
	})
	if err != nil {
		return RelayerApiContainer{}, fmt.Errorf("failed to create volume: %w", err)
	}
	volumeBinds := []string{fmt.Sprintf("%s:%s", v.Name, "/relayer")}
	if err := containerLifecycle.CreateContainer(ctx, docker.RunLabelValue, docker.NetworkID, image, portMap, volumeBinds, nil, "", cmd, env, []string{}); err != nil {
		return RelayerApiContainer{}, fmt.Errorf("failed to create container: %w", err)
	}

	json, err := config.GenerateConfigJSON()
	if err != nil {
		return RelayerApiContainer{}, fmt.Errorf("failed to generate config file: %w", err)
	}
	if err := dockerutil.NewFileWriter(log, docker.Client, docker.RunLabelValue).WriteFile(ctx, v.Name, "/relayer.json", []byte(json)); err != nil {
		return RelayerApiContainer{}, fmt.Errorf("failed to write config file: %w", err)
	}

	if err := containerLifecycle.StartContainer(ctx); err != nil {
		return RelayerApiContainer{}, fmt.Errorf("failed to start container: %w", err)
	}

	if err := containerLifecycle.CheckForFailedStart(ctx, 30*time.Second); err != nil {
		return RelayerApiContainer{}, fmt.Errorf("failed to check for failed start: %w", err)
	}

	ports, err := containerLifecycle.GetHostPorts(ctx, relayerPort)
	if err != nil {
		return RelayerApiContainer{}, fmt.Errorf("failed to get host ports: %w", err)
	}
	if len(ports) == 0 || ports[0] == "" {
		return RelayerApiContainer{}, errors.New("failed to get host port")
	}
	addr := ports[0]
	conn, err := grpc.NewClient(addr, grpc.WithTransportCredentials(insecure.NewCredentials()))
	if err != nil {
		return RelayerApiContainer{}, fmt.Errorf("failed to create grpc client: %w", err)
	}

	relayerServiceClient := NewRelayerServiceClient(conn)

	return RelayerApiContainer{
		containerLifecycle:   containerLifecycle,
		relayerServiceClient: relayerServiceClient,
	}, nil
}

func (r *RelayerApiContainer) GetCreateClientTx(ctx context.Context, srcChainID string, dstChainID string) ([]byte, error) {
	createClientRequest := &CreateClientRequest{
		SrcChain: srcChainID,
		DstChain: dstChainID,
	}
	createClientResponse, err := r.relayerServiceClient.CreateClient(ctx, createClientRequest)
	if err != nil {
		return nil, fmt.Errorf("failed to create client: %w", err)
	}
	return createClientResponse.Tx, nil
}

func (r *RelayerApiContainer) GetRelayTx(ctx context.Context, srcChainID string, dstChainID string, srcClientID string, dstClientID string, sourceTxHashes ...[]byte) ([]byte, error) {
	relayTxRequest := &RelayByTxRequest{
		SrcChain:    srcChainID,
		DstChain:    dstChainID,
		SrcClientId: srcClientID,
		DstClientId: dstClientID,
		SourceTxIds: sourceTxHashes,
	}
	relayTxResponse, err := r.relayerServiceClient.RelayByTx(ctx, relayTxRequest)
	if err != nil {
		return nil, fmt.Errorf("failed to relay tx: %w", err)
	}
	return relayTxResponse.Tx, nil
}

func (r *RelayerApiContainer) GetRelayTimeoutTx(ctx context.Context, srcChainID string, dstChainID string, srcClientID string, dstClientID string, timeoutTxHashses ...[]byte) ([]byte, error) {
	relayTimeoutRequest := &RelayByTxRequest{
		SrcChain:     srcChainID,
		DstChain:     dstChainID,
		SrcClientId:  srcClientID,
		DstClientId:  dstClientID,
		TimeoutTxIds: timeoutTxHashses,
	}
	relayTimeoutResponse, err := r.relayerServiceClient.RelayByTx(ctx, relayTimeoutRequest)
	if err != nil {
		return nil, fmt.Errorf("failed to relay timeout: %w", err)
	}
	return relayTimeoutResponse.Tx, nil
}

// Kill stops and removes the container
func (r *RelayerApiContainer) Kill() error {
	ctx := context.Background()

	// Stop the container
	err := r.containerLifecycle.StopContainer(ctx)
	if err != nil {
		return fmt.Errorf("failed to stop container: %w", err)
	}

	// Remove the container
	err = r.containerLifecycle.RemoveContainer(ctx)
	if err != nil {
		return fmt.Errorf("failed to remove container: %w", err)
	}

	return nil
}
