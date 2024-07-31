package operator

import (
	"os/exec"
	"runtime"
)

func BinaryPath() string {
	switch runtime.GOOS {
	case "darwin":
		return "e2e/artifacts/darwin-aarch64/operator"
	case "linux":
		return "e2e/artifacts/linux-x86_64/operator"
	default:
		panic("unsupported OS")
	}
}

// RunGenesis is a function that runs the genesis script to generate genesis.json
func RunGenesis(args ...string) error {
	args = append([]string{"genesis"}, args...)
	// nolint:gosec
	return exec.Command(BinaryPath(), args...).Run()
}

// StartOperator is a function that runs the operator
func StartOperator(args ...string) error {
	args = append([]string{"start"}, args...)
	// nolint:gosec
	return exec.Command(BinaryPath(), args...).Run()
}
