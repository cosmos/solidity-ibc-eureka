package operator

import "os/exec"

// RunGenesis is a function that runs the genesis script to generate genesis.json
func RunGenesis(args ...string) error {
	args = append([]string{"genesis"}, args...)
	return exec.Command("target/release/operator", args...).Run()
}

// StartOperator is a function that runs the operator
func StartOperator(args ...string) error {
	args = append([]string{"start"}, args...)
	return exec.Command("target/release/operator", args...).Run()
}
