operator_bin := if arch() == "x86_64" {
	if os() == "linux" {
		"./e2e/artifacts/linux-x86_64/operator"
	} else {
		error("unsupported OS")
	}
} else if arch() == "aarch64" {
	if os() == "macos" {
		"./e2e/artifacts/darwin-aarch64/operator"
	} else {
		error("unsupported OS")
	}
} else {
	error("unsupported architecture")
}

# Build the contracts using `forge build`
build:
	just clean
	forge build
 
# Clean up the cache and out directories
clean:
	@echo "Cleaning up cache and out directories"
	-rm -rf cache out # ignore errors

# Run the foundry tests
test-foundry:
	forge test -vvv

# Run forge fmt and bun solhint
lint:
	forge fmt --check && bun solhint '{script,src,test}/**/*.sol'

# Generate the ABI files for the contracts
generate-abi:
	just build
	jq '.abi' out/ICS26Router.sol/ICS26Router.json > abi/ICS26Router.json
	jq '.abi' out/ICS02Client.sol/ICS02Client.json > abi/ICS02Client.json   
	jq '.abi' out/ICS20Transfer.sol/ICS20Transfer.json > abi/ICS20Transfer.json
	jq '.abi' ./out/SP1ICS07Tendermint.sol/SP1ICS07Tendermint.json > abi/SP1ICS07Tendermint.json
	jq '.abi' out/ERC20.sol/ERC20.json > abi/ERC20.json
	abigen --abi abi/ICS02Client.json --pkg ics02client --type Contract --out e2e/interchaintestv8/types/ics02client/contract.go
	abigen --abi abi/ICS20Transfer.json --pkg ics20transfer --type Contract --out e2e/interchaintestv8/types/ics20transfer/contract.go
	abigen --abi abi/ICS26Router.json --pkg ics26router --type Contract --out e2e/interchaintestv8/types/ics26router/contract.go
	abigen --abi abi/SP1ICS07Tendermint.json --pkg sp1ics07tendermint --type Contract --out e2e/interchaintestv8/types/sp1ics07tendermint/contract.go
	abigen --abi abi/ERC20.json --pkg erc20 --type Contract --out e2e/interchaintestv8/types/erc20/contract.go
