package attestor

import (
	"context"
	"fmt"
	"os"
	"os/exec"
	"time"

	grpc "google.golang.org/grpc"
	insecure "google.golang.org/grpc/credentials/insecure"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types"
	attestortypes "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/ibc-attestor"
)

// StartAttestor starts the attestor with the given config file.
func StartAttestor(configPath string, binaryPath types.AttestorBinaryPath) (*exec.Cmd, error) {
	config, err := os.ReadFile(configPath)
	if err != nil {
		return nil, err
	}
	fmt.Printf("Starting attestor with config:\n%s\n", config)

	keyGenCmd := exec.Command(binaryPath, "key", "generate")
	keyGenCmd.Stdout = os.Stdout
	keyGenCmd.Stderr = os.Stderr
	// Ignore error as key might already exist
	_ = keyGenCmd.Run()

	cmd := exec.Command(binaryPath, "server", "--config", configPath)
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr

	err = cmd.Start()
	if err != nil {
		return nil, err
	}

	time.Sleep(2 * time.Second)

	return cmd, nil
}

// GetAttestationServiceClient returns an AttestationServiceClient for the attestor.
func GetAttestationServiceClient(addr string) (attestortypes.AttestationServiceClient, error) {
	conn, err := grpc.NewClient(addr, grpc.WithTransportCredentials(insecure.NewCredentials()))
	if err != nil {
		return nil, err
	}

	return attestortypes.NewAttestationServiceClient(conn), nil
}

// GetStateAttestation is a simple wrapper for the StateAttestation gRPC call.
func GetStateAttestation(ctx context.Context, client attestortypes.AttestationServiceClient, height uint64) (*attestortypes.StateAttestationResponse, error) {
	req := &attestortypes.StateAttestationRequest{
		Height: height,
	}
	return client.StateAttestation(ctx, req)
}

// GetPacketAttestation is a simple wrapper for the PacketAttestation gRPC call.
func GetPacketAttestation(ctx context.Context, client attestortypes.AttestationServiceClient, packets [][]byte) (*attestortypes.PacketAttestationResponse, error) {
	req := &attestortypes.PacketAttestationRequest{
		Packets: packets,
	}
	return client.PacketAttestation(ctx, req)
}
