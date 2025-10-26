package chainconfig

import (
	"context"
	"fmt"
	"os"
	"os/exec"
	"strings"
	"time"

	"github.com/gagliardetto/solana-go"
	"github.com/gagliardetto/solana-go/rpc"

	"github.com/srdtrk/solidity-ibc-eureka/e2e/v8/testvalues"
)

// SolanaChain runs Solana test validator in Docker
type SolanaChain struct {
	ContainerID   string
	ContainerName string
	RPCClient     *rpc.Client
	Faucet        *solana.Wallet
}

func StartSolanaDocker(ctx context.Context) (SolanaChain, error) {
	solanaChain := SolanaChain{}
	solanaChain.Faucet = solana.NewWallet()

	// Clean up any existing containers first
	cleanupCmd := exec.Command("docker", "ps", "-aq", "--filter", "name=solana-test-")
	if existingContainers, err := cleanupCmd.Output(); err == nil && len(existingContainers) > 0 {
		// Stop and remove existing containers
		stopCmd := exec.Command("sh", "-c", "docker stop $(docker ps -aq --filter 'name=solana-test-') 2>/dev/null || true")
		if err := stopCmd.Run(); err != nil {
			return SolanaChain{}, fmt.Errorf("failed to stop solana docker contrainer: %w", err)
		}
		rmCmd := exec.Command("sh", "-c", "docker rm -f $(docker ps -aq --filter 'name=solana-test-') 2>/dev/null || true")
		if err := rmCmd.Run(); err != nil {
			return SolanaChain{}, fmt.Errorf("failed to remove solana docker contrainer: %w", err)
		}
	}

	dockerImage := "beeman/solana-test-validator:latest"
	containerName := fmt.Sprintf("solana-test-%d", time.Now().Unix())

	// Start the container with a pre-funded faucet account
	// The --faucet-sol flag gives the account initial balance
	args := []string{
		"run", "-d",
		"--rm", // Auto-remove container when it stops
		"--name", containerName,
		"-p", "8899:8899", // RPC port
		"-p", "8900:8900", // WebSocket port
		"-p", "9900:9900", // Faucet port
		dockerImage,
		"solana-test-validator",
		"--reset",
		"--mint", solanaChain.Faucet.PublicKey().String(),
		"--faucet-sol", "1000000", // Give the faucet account 1M SOL
	}

	cmd := exec.CommandContext(ctx, "docker", args...)
	output, err := cmd.CombinedOutput()
	if err != nil {
		return SolanaChain{}, fmt.Errorf("failed to start Solana docker container: %w, output: %s", err, string(output))
	}

	// Trim whitespace and take first 12 chars of container ID
	containerIDStr := strings.TrimSpace(string(output))
	if len(containerIDStr) >= 12 {
		solanaChain.ContainerID = containerIDStr[:12]
	} else {
		solanaChain.ContainerID = containerIDStr
	}
	solanaChain.ContainerName = containerName

	// Wait for the Solana validator to be ready
	time.Sleep(8 * time.Second)

	// Health check for RPC endpoint
	for range 10 {
		healthCmd := exec.CommandContext(ctx, "docker", "exec", containerName,
			"curl", "-s", "http://localhost:8899", "-X", "POST",
			"-H", "Content-Type: application/json",
			"-d", `{"jsonrpc":"2.0","id":1,"method":"getHealth"}`)
		if output, err := healthCmd.Output(); err == nil && len(output) > 0 {
			break
		}
		time.Sleep(1 * time.Second)
	}

	// Health check for WebSocket endpoint - ensure port 8900 is listening
	for range 10 {
		wsCheckCmd := exec.CommandContext(ctx, "docker", "exec", containerName,
			"sh", "-c", "nc -z localhost 8900")
		if err := wsCheckCmd.Run(); err == nil {
			break
		}
		time.Sleep(1 * time.Second)
	}

	solanaChain.RPCClient = rpc.New(rpc.LocalNet.RPC)

	return solanaChain, nil
}

func (s SolanaChain) Destroy() error {
	if s.ContainerID != "" {
		// Container might already be stopped, continue with removal
		stopCmd := exec.Command("docker", "stop", s.ContainerID)
		_ = stopCmd.Run()

		// Remove the container
		rmCmd := exec.Command("docker", "rm", "-f", s.ContainerID)
		if err := rmCmd.Run(); err != nil {
			return fmt.Errorf("failed to remove Solana docker container: %w", err)
		}
	}

	// Clean up ledger directory if it exists (directory might not exist, ignore error)
	_ = os.RemoveAll(testvalues.SolanaLedgerDir)

	return nil
}
