package attestor

import (
	"bytes"
	"context"
	"fmt"
	"os"
	"os/exec"
	"strings"
	"time"

	grpc "google.golang.org/grpc"
	insecure "google.golang.org/grpc/credentials/insecure"

	attestortypes "github.com/srdtrk/solidity-ibc-eureka/e2e/v8/types/attestor"
)

type AttestorBinaryPath = string

const (
	OptimismBinary AttestorBinaryPath = "ibc_op_attestor"
	ArbitrumBinary AttestorBinaryPath = "ibc_arbitrum_attestor"
	CosmosBinary   AttestorBinaryPath = "ibc_cosmos_attestor"
)

// StartAttestor starts the attestor with the given config file.
func StartAttestor(configPath string, binaryPath AttestorBinaryPath) (*os.Process, error) {
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

	time.Sleep(5 * time.Second)

	return cmd.Process, nil
}

func ReadAttestorAddress(binaryPath AttestorBinaryPath) (string, error) {
	cmd := exec.Command(binaryPath, "key", "show")
	var stdout bytes.Buffer
	var stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr
	if err := cmd.Run(); err != nil {
		return "", fmt.Errorf("failed to read attestor address: %v: %s", err, stderr.String())
	}
	return strings.TrimSpace(stdout.String()), nil
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
func GetPacketAttestation(ctx context.Context, client attestortypes.AttestationServiceClient, packets [][]byte, height uint64) (*attestortypes.PacketAttestationResponse, error) {
	req := &attestortypes.PacketAttestationRequest{
		Packets: packets,
		Height:  height,
	}
	return client.PacketAttestation(ctx, req)
}
