package relayer

import (
	"fmt"
	"os"
	"os/exec"
	"time"

	grpc "google.golang.org/grpc"
	insecure "google.golang.org/grpc/credentials/insecure"

	relayertypes "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/relayer"
)

// DefaultRelayerGRPCAddress returns the default gRPC address for the relayer.
func DefaultRelayerGRPCAddress() string {
	return "127.0.0.1:3000"
}

// binaryPath returns the path to the relayer binary.
func binaryPath() string {
	return "relayer"
}

// StartRelayer starts the relayer with the given config file.
func StartRelayer(configPath string) (*os.Process, error) {
	config, err := os.ReadFile(configPath)
	if err != nil {
		return nil, err
	}
	fmt.Printf("Starting relayer with config:\n%s\n", config)

	cmd := exec.Command(binaryPath(), "start", "--config", configPath)
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr

	// run this command in the background
	err = cmd.Start()
	if err != nil {
		return nil, err
	}

	// wait for the relayer to start
<<<<<<< HEAD
	time.Sleep(5 * time.Second)
=======
	time.Sleep(9 * time.Second)
>>>>>>> 5a7e361 (imp(eth-lc): add support for fusaka/fulu hard fork (#799))

	return cmd.Process, nil
}

// GetGRPCClient returns a gRPC client for the relayer.
func GetGRPCClient(addr string) (relayertypes.RelayerServiceClient, error) {
	conn, err := grpc.NewClient(addr, grpc.WithTransportCredentials(insecure.NewCredentials()))
	if err != nil {
		return nil, err
	}

	return relayertypes.NewRelayerServiceClient(conn), nil
}
