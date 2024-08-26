set dotenv-load

# Use the SP1_OPERATOR_REV environment variable if it is set, otherwise use a default commit hash
sp1_operator_rev := env_var_or_default('SP1_OPERATOR_REV', '8b8813e636df8825ff45f4410e619a95f2f8ae5a')

# Build the contracts using `forge build`
build:
	just clean
	forge build
 
# Clean up the cache and out directories
clean:
	@echo "Cleaning up cache and out directories"
	-rm -rf cache out # ignore errors

# Run the foundry tests
test-foundry testname=".\\*":
	forge test -vvv --match-test ^{{testname}}\(.\*\)\$

# Run the benchmark tests
test-benchmark:
	forge test -vvv --gas-report --match-path test/BenchmarkTest.t.sol

# Run forge fmt and bun solhint
lint:
	@echo "Linting the Solidity code..."
	forge fmt --check && bun solhint -w 0 '{script,src,test}/**/*.sol' && bun natspec-smells --include 'src/**/*.sol'
	@echo "Linting the Go code..."
	cd e2e/interchaintestv8 && golangci-lint run --fix

# Generate the ABI files for the contracts
generate-abi:
	just build
	jq '.abi' out/ICS26Router.sol/ICS26Router.json > abi/ICS26Router.json
	jq '.abi' out/ICS02Client.sol/ICS02Client.json > abi/ICS02Client.json   
	jq '.abi' out/ICS20Transfer.sol/ICS20Transfer.json > abi/ICS20Transfer.json
	jq '.abi' ./out/SP1ICS07Tendermint.sol/SP1ICS07Tendermint.json > abi/SP1ICS07Tendermint.json
	jq '.abi' out/ERC20.sol/ERC20.json > abi/ERC20.json
	jq '.abi' out/IBCERC20.sol/IBCERC20.json > abi/IBCERC20.json
	abigen --abi abi/ICS02Client.json --pkg ics02client --type Contract --out e2e/interchaintestv8/types/ics02client/contract.go
	abigen --abi abi/ICS20Transfer.json --pkg ics20Transfer --type Contract --out e2e/interchaintestv8/types/ics20Transfer/contract.go
	abigen --abi abi/ICS26Router.json --pkg ics26router --type Contract --out e2e/interchaintestv8/types/ics26router/contract.go
	abigen --abi abi/SP1ICS07Tendermint.json --pkg sp1ics07tendermint --type Contract --out e2e/interchaintestv8/types/sp1ics07tendermint/contract.go
	abigen --abi abi/ERC20.json --pkg erc20 --type Contract --out e2e/interchaintestv8/types/erc20/contract.go
	abigen --abi abi/IBCERC20.json --pkg ibcerc20 --type Contract --out e2e/interchaintestv8/types/ibcerc20/contract.go

# Run the e2e tests
test-e2e testname:
	just clean
	@echo "Running {{testname}} test..."
	cd e2e/interchaintestv8 && go test -v -run '^TestWithIbcEurekaTestSuite/{{testname}}$' -timeout 40m

# Install the sp1-ics07-tendermint operator for use in the e2e tests
install-operator:
	cargo install --git https://github.com/cosmos/sp1-ics07-tendermint --rev {{sp1_operator_rev}} sp1-ics07-tendermint-operator --bin operator --locked

# Generate the fixtures for the Solidity tests using the e2e tests
generate-fixtures:
	@echo "Generating fixtures... This may take a while."
	just clean
	@echo "Generating recvPacket and acknowledgePacket fixtures..."
	cd e2e/interchaintestv8 && GENERATE_FIXTURES=true SP1_PROVER=network go test -v -run '^TestWithIbcEurekaTestSuite/TestICS20TransferERC20TokenfromEthereumToCosmosAndBack$' -timeout 40m
	@echo "Generating native SdkCoin recvPacket fixtures..."
	cd e2e/interchaintestv8 && GENERATE_FIXTURES=true SP1_PROVER=network go test -v -run '^TestWithIbcEurekaTestSuite/TestICS20TransferNativeCosmosCoinsToEthereumAndBack$' -timeout 40m
	@echo "Generating timeoutPacket fixtures..."
	cd e2e/interchaintestv8 && GENERATE_FIXTURES=true SP1_PROVER=network go test -v -run '^TestWithIbcEurekaTestSuite/TestICS20TransferTimeoutFromEthereumToCosmosChain$' -timeout 40m
