package aggregator

import (
	"context"
	"fmt"
	"os"
	"os/exec"
	"time"

	grpc "google.golang.org/grpc"
	insecure "google.golang.org/grpc/credentials/insecure"

	aggregatortypes "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/aggregator"
)

type AggregatorBinaryPath = string

const (
	AggregatorBinary AggregatorBinaryPath = "aggregator"
)

// StartAggregator starts the aggregator with the given config file and attestor endpoints
func StartAggregator(configPath string, binaryPath AggregatorBinaryPath) (*os.Process, error) {
	config, err := os.ReadFile(configPath)
	if err != nil {
		return nil, err
	}
	fmt.Printf("Starting aggregator with config:\n%s\n", config)

	cmd := exec.Command(binaryPath, "server", "--config", configPath)
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr

	err = cmd.Start()
	if err != nil {
		return nil, err
	}

	time.Sleep(5 * time.Second)

	return cmd.Process, nil
}

// GetAggregatorServiceClient returns an AggregatorServiceClient for the aggregator.
func GetAggregatorServiceClient(addr string) (aggregatortypes.AggregatorServiceClient, error) {
	conn, err := grpc.NewClient(addr, grpc.WithTransportCredentials(insecure.NewCredentials()))
	if err != nil {
		return nil, err
	}

	return aggregatortypes.NewAggregatorServiceClient(conn), nil
}

// GetAttestations is a simple wrapper for the GetAttestations gRPC call.
func GetAttestations(ctx context.Context, client aggregatortypes.AggregatorServiceClient, packets [][]byte, height uint64) (*aggregatortypes.GetAttestationsResponse, error) {
	req := &aggregatortypes.GetAttestationsRequest{
		Packets: packets,
		Height:  height,
	}
	return client.GetAttestations(ctx, req)
}
