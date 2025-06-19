package chainconfig

import (
	"context"
	"fmt"
	"os"
	"os/exec"
	"time"
)

const (
	nitroTestnodeDir = "nitro-testnode"
)

type TestnodeArbitrumChain struct {
	ExecutionRPC string
	ConsensusRPC string
	// Faucet       *ecdsa.PrivateKey // Ignore for now!

	// Internal fields for cleanup
	projectDir  string
	projectName string
}

func SpinUpTestnodeArbitrum(ctx context.Context) (TestnodeArbitrumChain, error) {
	// Check if the directory already exists
	if _, err := os.Stat("nitro-testnode"); os.IsNotExist(err) {
		// Clone the nitro-testnode repository
		cloneCmd := exec.CommandContext(ctx, "git", "clone", "-b", "release", "--recurse-submodules",
			"https://github.com/OffchainLabs/nitro-testnode.git")
		cloneCmd.Stdout = os.Stdout
		cloneCmd.Stderr = os.Stderr

		if err := cloneCmd.Run(); err != nil {
			return TestnodeArbitrumChain{}, fmt.Errorf("failed to clone nitro-testnode: %w", err)
		}

		if err := os.Chdir(nitroTestnodeDir); err != nil {
			return TestnodeArbitrumChain{}, fmt.Errorf("failed to change to testnode directory: %w", err)
		}
	} else {
		// Directory exists, pull latest changes
		if err := os.Chdir(nitroTestnodeDir); err != nil {
			return TestnodeArbitrumChain{}, fmt.Errorf("failed to change to testnode directory: %w", err)
		}

		pullCmd := exec.CommandContext(ctx, "git", "pull", "origin", "release")
		pullCmd.Stdout = os.Stdout
		pullCmd.Stderr = os.Stderr

		if err := pullCmd.Run(); err != nil {
			return TestnodeArbitrumChain{}, fmt.Errorf("failed to pull latest changes: %w", err)
		}
	}

	// Always bring down any previous docker-compose project before starting
	dockerComposeDown(ctx, nitroTestnodeDir)

	// Start the testnode with docker-compose - print output in real-time
	fmt.Println("Starting test-node.bash...")

	// Create a simple expect script to handle TTY issues
	expectScript := `#!/usr/bin/expect -f
set timeout -1
spawn ./test-node.bash --init --no-simple --detach
expect {
    "y/n" { send "y\r" }
    "Y/n" { send "y\r" }
    "yes/no" { send "yes\r" }
    "YES/NO" { send "YES\r" }
    timeout { exit 1 }
}
expect eof
`

	// Write the expect script to a temporary file
	scriptPath := "./run-testnode.exp"
	if err := os.WriteFile(scriptPath, []byte(expectScript), 0755); err != nil {
		dockerComposeDown(ctx, nitroTestnodeDir)
		return TestnodeArbitrumChain{}, fmt.Errorf("failed to write expect script: %w", err)
	}

	startCmd := exec.CommandContext(ctx, scriptPath)
	startCmd.Stdout = os.Stdout
	startCmd.Stderr = os.Stderr

	if err := startCmd.Run(); err != nil {
		fmt.Printf("test-node.bash failed with error: %v\n", err)
		dockerComposeDown(ctx, nitroTestnodeDir)
		return TestnodeArbitrumChain{}, fmt.Errorf("failed to start testnode: %w", err)
	}

	fmt.Println("test-node.bash completed successfully")

	// Wait a bit for services to start up
	time.Sleep(15 * time.Second)

	// TODO: Not entierly sure which one is the correct one to use for execution and consensus here
	consensusRPC := "http://localhost:8547"
	executionRPC := "http://localhost:8547"

	// Verify the service is running by checking if we can connect
	// We'll use a simple curl command to test the connection
	testCmd := exec.CommandContext(ctx, "curl", "-s", "-X", "POST", "-H", "Content-Type: application/json",
		"-d", `{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}`, consensusRPC)

	if err := testCmd.Run(); err != nil {
		dockerComposeDown(ctx, nitroTestnodeDir)
		return TestnodeArbitrumChain{}, fmt.Errorf("failed to verify RPC connection: %w", err)
	}

	return TestnodeArbitrumChain{
		ExecutionRPC: executionRPC,
		ConsensusRPC: consensusRPC, // Same as ExecutionRPC for Arbitrum testnode
		projectDir:   nitroTestnodeDir,
		projectName:  nitroTestnodeDir,
	}, nil
}

func (e TestnodeArbitrumChain) Destroy(ctx context.Context) {
	dockerComposeDown(ctx, nitroTestnodeDir)
}

func (e TestnodeArbitrumChain) DumpLogs(ctx context.Context) error {
	if e.projectDir == "" {
		return fmt.Errorf("no project directory available")
	}

	// Change to the project directory
	if err := os.Chdir(e.projectDir); err != nil {
		return fmt.Errorf("failed to change to project directory: %w", err)
	}

	// Get logs from the geth service
	fmt.Println("=== Geth Service Logs ===")
	gethCmd := exec.CommandContext(ctx, "docker-compose", "logs", "geth")
	gethCmd.Stdout = os.Stdout
	gethCmd.Stderr = os.Stderr

	if err := gethCmd.Run(); err != nil {
		fmt.Printf("Warning: failed to get geth logs: %v\n", err)
	}

	// Get logs from the sequencer service
	fmt.Println("\n=== Sequencer Service Logs ===")
	sequencerCmd := exec.CommandContext(ctx, "docker-compose", "logs", "sequencer")
	sequencerCmd.Stdout = os.Stdout
	sequencerCmd.Stderr = os.Stderr

	if err := sequencerCmd.Run(); err != nil {
		fmt.Printf("Warning: failed to get sequencer logs: %v\n", err)
	}

	return nil
}

func dockerComposeDown(ctx context.Context, dir string) {
	downCmd := exec.CommandContext(ctx, "docker-compose", "down", "-v")
	downCmd.Dir = dir
	downCmd.Stdout = os.Stdout
	downCmd.Stderr = os.Stderr
	_ = downCmd.Run()
}
