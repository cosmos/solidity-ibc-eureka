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
