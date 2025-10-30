set dotenv-load

# Detect which anchor command is available
anchor_cmd := `command -v anchor-nix >/dev/null 2>&1 && echo "anchor-nix" || echo "anchor"`

# Default task lists all available tasks
default:
  just --list

# Build the contracts using `forge build`
[group('build')]
build-contracts: clean-foundry
	forge build

# Build the relayer using `cargo build`
[group('build')]
build-relayer:
	cargo build --bin relayer --release --locked

# Build the operator using `cargo build`
[group('build')]
build-operator:
	cargo build --bin operator --release --locked

# Build riscv elf files using `~/.sp1/bin/cargo-prove`
[group('build')]
build-sp1-programs:
  @echo "Building SP1 programs in 'programs/sp1-programs/target/elf-compilation/riscv32im-succinct-zkvm-elf/release/'"
  cd programs/sp1-programs && ~/.sp1/bin/cargo-prove prove build -p sp1-ics07-tendermint-update-client --locked
  cd programs/sp1-programs && ~/.sp1/bin/cargo-prove prove build -p sp1-ics07-tendermint-membership --locked
  cd programs/sp1-programs && ~/.sp1/bin/cargo-prove prove build -p sp1-ics07-tendermint-uc-and-membership --locked
  cd programs/sp1-programs && ~/.sp1/bin/cargo-prove prove build -p sp1-ics07-tendermint-misbehaviour --locked

# Sync Solana program keypairs and update declare_id! macros
# Usage: just sync-solana-keys [cluster]
# Example: just sync-solana-keys devnet
[group('build')]
sync-solana-keys cluster="localnet":
  @echo "Syncing Solana program keys for cluster: {{cluster}}"
  @if [ ! -d "solana-keypairs/{{cluster}}" ]; then \
    echo "❌ Cluster directory not found: solana-keypairs/{{cluster}}"; \
    echo "   Available clusters: localnet, devnet, testnet, mainnet"; \
    exit 1; \
  fi
  @if [ "{{cluster}}" != "localnet" ] && [ ! -f "solana-keypairs/{{cluster}}/ics26_router-keypair.json" ]; then \
    echo "❌ No keypairs found for cluster: {{cluster}}"; \
    echo "   Generate them first with: just generate-solana-keypairs {{cluster}}"; \
    exit 1; \
  fi
  @echo "Copying keypairs from solana-keypairs/{{cluster}}/ to target/deploy/..."
  mkdir -p programs/solana/target/deploy
  cp -f solana-keypairs/{{cluster}}/*-keypair.json programs/solana/target/deploy/ 2>/dev/null || true
  @echo "Syncing declare_id! and Anchor.toml sections..."
  @echo "🦀 Using {{anchor_cmd}}"
  (cd programs/solana && {{anchor_cmd}} keys sync --provider.cluster {{cluster}})
  @echo "✅ Keys synced for cluster: {{cluster}}"

# Build Solana Anchor programs for a specific cluster (default: localnet)
# Usage: just build-solana [cluster]
# Example: just build-solana mainnet
[group('build')]
build-solana cluster="localnet": (sync-solana-keys cluster)
  @echo "Building programs..."
  @echo "🦀 Using {{anchor_cmd}}"
  (cd programs/solana && {{anchor_cmd}} build)
  @echo "✅ Build complete for cluster: {{cluster}}"

# Deploy Solana Anchor programs to a specific cluster (default: localnet)
# Usage: just deploy-solana [cluster]
# Example: just deploy-solana devnet
[group('build')]
deploy-solana cluster="localnet": (build-solana cluster)
  @echo "Deploying programs to {{cluster}}..."
  @echo "🚀 Using {{anchor_cmd}}"
  (cd programs/solana && {{anchor_cmd}} deploy --provider.cluster {{cluster}})
  @echo "✅ Deployment complete for cluster: {{cluster}}"

# Generate Solana keypairs for a specific cluster
# Usage: just generate-solana-keypairs <cluster>
# Example: just generate-solana-keypairs devnet
[group('build')]
generate-solana-keypairs cluster:
  @if [ "{{cluster}}" = "localnet" ]; then \
    echo "❌ Cannot generate keypairs for localnet - they are tracked in git"; \
    echo "   Localnet keypairs are used for E2E tests and should not be regenerated"; \
    exit 1; \
  fi
  @if [ ! -d "solana-keypairs/{{cluster}}" ]; then \
    echo "❌ Unknown cluster: {{cluster}}"; \
    echo "   Available clusters: devnet, testnet, mainnet"; \
    exit 1; \
  fi
  @echo "Generating keypairs for cluster: {{cluster}}"
  solana-keygen new --no-bip39-passphrase --force --outfile solana-keypairs/{{cluster}}/ics26_router-keypair.json
  solana-keygen new --no-bip39-passphrase --force --outfile solana-keypairs/{{cluster}}/ics07_tendermint-keypair.json
  solana-keygen new --no-bip39-passphrase --force --outfile solana-keypairs/{{cluster}}/ics27_gmp-keypair.json
  solana-keygen new --no-bip39-passphrase --force --outfile solana-keypairs/{{cluster}}/gmp_counter_app-keypair.json
  @echo ""
  @echo "✅ Keypairs generated in solana-keypairs/{{cluster}}/"
  @echo "⚠️  IMPORTANT: Backup these keypairs securely! They are NOT tracked in git."
  @echo ""
  @echo "📋 Program IDs for {{cluster}}:"
  @for keypair in solana-keypairs/{{cluster}}/*-keypair.json; do \
    printf "   %-35s %s\n" "$$(basename $$keypair):" "$$(solana-keygen pubkey $$keypair)"; \
  done
  @echo ""
  @echo "Next step: just build-solana {{cluster}}"

# Build and optimize the eth wasm light client using a local docker image. Requires `docker` and `gzip`
[group('build')]
build-cw-ics08-wasm-eth:
  -@docker image rm cosmwasm-builder:latest
  cd programs/cw-ics08-wasm-eth && docker buildx build --platform linux/amd64 -t cosmwasm-builder:latest .
  docker run --rm --platform=linux/amd64  -t \
    -v "$PWD":/code \
    cosmwasm-builder:latest
  cp artifacts/cw_ics08_wasm_eth.wasm e2e/interchaintestv8/wasm
  gzip -n e2e/interchaintestv8/wasm/cw_ics08_wasm_eth.wasm -f

# Build the relayer docker image
# Only for linux/amd64 since sp1 doesn't have an arm image built
[group('build')]
build-relayer-image:
    docker build -t eureka-relayer:latest -f programs/relayer/Dockerfile .

# Install the sp1-ics07-tendermint operator for use in the e2e tests
[group('install')]
install-operator:
	cargo install --bin operator --path programs/operator --locked

# Install the relayer using `cargo install`
[group('install')]
install-relayer:
	cargo install --bin relayer --path programs/relayer --locked

# Run all linters
[group('lint')]
lint:
	@echo "Running all linters..."
	just lint-solidity
	just lint-go
	just lint-buf
	just lint-rust
	just lint-solana

# Lint the Solidity code using `forge fmt` and `bun:solhint`
[group('lint')]
lint-solidity:
	@echo "Linting the Solidity code..."
	forge fmt --check
	bun solhint -w 0 '{scripts,contracts,test}/**/*.sol'
	natlint run --include 'contracts/**/*.sol'

# Lint the Go code using `golangci-lint`
[group('lint')]
lint-go:
	@echo "Linting the Go code..."
	cd e2e/interchaintestv8 && golangci-lint run
	cd packages/go-abigen && golangci-lint run
	cd packages/go-anchor && golangci-lint run

# Lint the Protobuf files using `buf lint`
[group('lint')]
lint-buf:
	@echo "Linting the Protobuf files..."
	buf lint

# Lint the Rust code using `cargo fmt` and `cargo clippy`
[group('lint')]
lint-rust:
	@echo "Linting the Rust code..."
	cargo fmt --all -- --check
	cargo clippy --all-targets --all-features -- -D warnings
	cd programs/sp1-programs && cargo fmt --all -- --check
	cd programs/sp1-programs && cargo clippy --all-targets --all-features -- -D warnings

# Lint the Solana code using `cargo fmt` and `cargo clippy`
[group('lint')]
lint-solana:
	@echo "Linting the Solana code..."
	cd programs/solana && cargo fmt --all -- --check
	cd programs/solana && cargo +nightly clippy --all-targets --all-features -- -D warnings


# Generate the (non-bytecode) ABI files for the contracts
[group('generate')]
generate-abi: build-contracts
	jq '.abi' out/ICS26Router.sol/ICS26Router.json > abi/ICS26Router.json
	jq '.abi' out/ICS20Transfer.sol/ICS20Transfer.json > abi/ICS20Transfer.json
	jq '.abi' out/SP1ICS07Tendermint.sol/SP1ICS07Tendermint.json > abi/SP1ICS07Tendermint.json
	jq '.abi' out/ERC20.sol/ERC20.json > abi/ERC20.json
	jq '.abi' out/IBCERC20.sol/IBCERC20.json > abi/IBCERC20.json
	jq '.abi' out/RelayerHelper.sol/RelayerHelper.json > abi/RelayerHelper.json
	abigen --abi abi/ERC20.json --pkg erc20 --type Contract --out e2e/interchaintestv8/types/erc20/contract.go
	abigen --abi abi/SP1ICS07Tendermint.json --pkg sp1ics07tendermint --type Contract --out packages/go-abigen/sp1ics07tendermint/contract.go
	abigen --abi abi/ICS20Transfer.json --pkg ics20transfer --type Contract --out packages/go-abigen/ics20transfer/contract.go
	abigen --abi abi/ICS26Router.json --pkg ics26router --type Contract --out packages/go-abigen/ics26router/contract.go
	abigen --abi abi/IBCERC20.json --pkg ibcerc20 --type Contract --out packages/go-abigen/ibcerc20/contract.go
	abigen --abi abi/RelayerHelper.json --pkg relayerhelper --type Contract --out packages/go-abigen/relayerhelper/contract.go

# Generate the ABI files with bytecode for the required contracts (only SP1ICS07Tendermint)
[group('generate')]
generate-abi-bytecode: build-contracts
	cp out/SP1ICS07Tendermint.sol/SP1ICS07Tendermint.json abi/bytecode

# Generate the types for interacting with SVM contracts using 'anchor-go'
[group('generate')]
generate-solana-types: build-solana generate-pda
	@echo "Generating SVM types..."
	rm -rf packages/go-anchor/ics07tendermint
	anchor-go --idl ./programs/solana/target/idl/ics07_tendermint.json --output packages/go-anchor/ics07tendermint --no-go-mod
	rm -rf packages/go-anchor/ics26router
	anchor-go --idl ./programs/solana/target/idl/ics26_router.json --output packages/go-anchor/ics26router --no-go-mod
	rm -rf packages/go-anchor/ics27gmp
	anchor-go --idl ./programs/solana/target/idl/ics27_gmp.json --output packages/go-anchor/ics27gmp --no-go-mod
	rm -rf packages/go-anchor/dummyibcapp
	anchor-go --idl ./programs/solana/target/idl/dummy_ibc_app.json --output packages/go-anchor/dummyibcapp --no-go-mod
	rm -rf e2e/interchaintestv8/solana/go-anchor/mocklightclient
	anchor-go --idl ./programs/solana/target/idl/mock_light_client.json --output e2e/interchaintestv8/solana/go-anchor/mocklightclient --no-go-mod
	rm -rf e2e/interchaintestv8/solana/go-anchor/gmpcounter
	anchor-go --idl ./programs/solana/target/idl/gmp_counter_app.json --output e2e/interchaintestv8/solana/go-anchor/gmpcounter --no-go-mod

# Generate Solana PDA helpers from Anchor IDL files
[group('generate')]
generate-pda:
	@echo "Generating Solana PDA helpers from Anchor IDL..."
	go run e2e/interchaintestv8/solana/generate-pdas/main.go \
		--idl-dir programs/solana/target/idl \
		--output e2e/interchaintestv8/solana/pda.go
	@echo "✅ Generated e2e/interchaintestv8/solana/pda.go"

# Generate the fixtures for the wasm tests using the e2e tests
[group('generate')]
generate-fixtures-wasm: clean-foundry install-relayer
	@echo "Generating fixtures... This may take a while."
	@echo "Generating recvPacket and acknowledgePacket groth16 fixtures..."
	cd e2e/interchaintestv8 && ETH_TESTNET_TYPE=pos GENERATE_WASM_FIXTURES=true E2E_PROOF_TYPE=groth16 go test -v -run '^TestWithIbcEurekaTestSuite/Test_ICS20TransferERC20TokenfromEthereumToCosmosAndBack$' -timeout 60m
	@echo "Generating native SdkCoin recvPacket groth16 fixtures..."
	cd e2e/interchaintestv8 && ETH_TESTNET_TYPE=pos GENERATE_WASM_FIXTURES=true E2E_PROOF_TYPE=groth16 go test -v -run '^TestWithIbcEurekaTestSuite/Test_ICS20TransferNativeCosmosCoinsToEthereumAndBack$' -timeout 60m
	@echo "Generating timeoutPacket groth16 fixtures..."
	cd e2e/interchaintestv8 && ETH_TESTNET_TYPE=pos GENERATE_WASM_FIXTURES=true E2E_PROOF_TYPE=groth16 go test -v -run '^TestWithIbcEurekaTestSuite/Test_TimeoutPacketFromCosmos$' -timeout 60m
	@echo "Generating multi-period client update fixtures..."
	cd e2e/interchaintestv8 && ETH_TESTNET_TYPE=pos GENERATE_WASM_FIXTURES=true go test -v -run '^TestWithRelayerTestSuite/Test_MultiPeriodClientUpdateToCosmos$' -timeout 60m

# Generate the fixtures for the Tendermint light client tests using the e2e tests
[group('generate')]
generate-fixtures-tendermint-light-client: install-relayer
	@echo "Generating Tendermint light client fixtures... This may take a while."
	@echo "Generating basic membership and update client fixtures..."
	cd e2e/interchaintestv8 && GENERATE_TENDERMINT_LIGHT_CLIENT_FIXTURES=true go test -v -run '^TestWithCosmosRelayerTestSuite/Test_UpdateClient$' -timeout 40m

# Generate go types for the e2e tests from the etheruem light client code
[group('generate')]
generate-ethereum-types:
	cargo run --bin generate_json_schema --features test-utils
	bun quicktype --src-lang schema --lang go --just-types-and-package --package ethereum --src ethereum_types_schema.json --out e2e/interchaintestv8/types/ethereum/types.gen.go --top-level GeneratedTypes
	rm ethereum_types_schema.json
	sed -i.bak 's/int64/uint64/g' e2e/interchaintestv8/types/ethereum/types.gen.go # quicktype generates int64 instead of uint64 :(
	rm -f e2e/interchaintestv8/types/ethereum/types.gen.go.bak # this is to be linux and mac compatible (coming from the sed command)
	cd e2e/interchaintestv8 && golangci-lint run --fix types/ethereum/types.gen.go

# Generate the fixtures for the Solidity tests using the e2e tests
[group('generate')]
generate-fixtures-solidity: clean-foundry install-operator install-relayer
	@echo "Generating fixtures... This may take a while."
	@echo "Generating recvPacket and acknowledgePacket groth16 fixtures..."
	cd e2e/interchaintestv8 && GENERATE_SOLIDITY_FIXTURES=true SP1_PROVER=network E2E_PROOF_TYPE=groth16 go test -v -run '^TestWithIbcEurekaTestSuite/Test_ICS20TransferERC20TokenfromEthereumToCosmosAndBack$' -timeout 40m
	@echo "Generating recvPacket and acknowledgePacket plonk fixtures..."
	cd e2e/interchaintestv8 && GENERATE_SOLIDITY_FIXTURES=true SP1_PROVER=network E2E_PROOF_TYPE=plonk go test -v -run '^TestWithIbcEurekaTestSuite/Test_ICS20TransferERC20TokenfromEthereumToCosmosAndBack$' -timeout 40m
	@echo "Generating recvPacket and acknowledgePacket groth16 fixtures for 25 packets..."
	cd e2e/interchaintestv8 && GENERATE_SOLIDITY_FIXTURES=true SP1_PROVER=network E2E_PROOF_TYPE=groth16 go test -v -run '^TestWithIbcEurekaTestSuite/Test_25_ICS20TransferERC20TokenfromEthereumToCosmosAndBack$' -timeout 40m
	@echo "Generating recvPacket and acknowledgePacket groth16 fixtures for 50 packets..."
	cd e2e/interchaintestv8 && GENERATE_SOLIDITY_FIXTURES=true SP1_PROVER=network E2E_PROOF_TYPE=groth16 go test -v -run '^TestWithIbcEurekaTestSuite/Test_50_ICS20TransferERC20TokenfromEthereumToCosmosAndBack$' -timeout 40m
	@echo "Generating recvPacket and acknowledgePacket plonk fixtures for 50 packets..."
	cd e2e/interchaintestv8 && GENERATE_SOLIDITY_FIXTURES=true SP1_PROVER=network E2E_PROOF_TYPE=plonk go test -v -run '^TestWithIbcEurekaTestSuite/Test_50_ICS20TransferERC20TokenfromEthereumToCosmosAndBack$' -timeout 40m
	@echo "Generating native SdkCoin recvPacket groth16 fixtures..."
	cd e2e/interchaintestv8 && GENERATE_SOLIDITY_FIXTURES=true SP1_PROVER=network E2E_PROOF_TYPE=groth16 go test -v -run '^TestWithIbcEurekaTestSuite/Test_ICS20TransferNativeCosmosCoinsToEthereumAndBack$' -timeout 40m
	@echo "Generating native SdkCoin recvPacket plonk fixtures..."
	cd e2e/interchaintestv8 && GENERATE_SOLIDITY_FIXTURES=true SP1_PROVER=network E2E_PROOF_TYPE=plonk go test -v -run '^TestWithIbcEurekaTestSuite/Test_ICS20TransferNativeCosmosCoinsToEthereumAndBack$' -timeout 40m
	@echo "Generating timeoutPacket groth16 fixtures..."
	cd e2e/interchaintestv8 && GENERATE_SOLIDITY_FIXTURES=true SP1_PROVER=network E2E_PROOF_TYPE=groth16 go test -v -run '^TestWithIbcEurekaTestSuite/Test_TimeoutPacketFromEth$' -timeout 40m
	@echo "Generating timeoutPacket plonk fixtures..."
	cd e2e/interchaintestv8 && GENERATE_SOLIDITY_FIXTURES=true SP1_PROVER=network E2E_PROOF_TYPE=plonk go test -v -run '^TestWithIbcEurekaTestSuite/Test_TimeoutPacketFromEth$' -timeout 40m

private_cluster := if env("E2E_PRIVATE_CLUSTER", "") == "true" { "--private-cluster" } else { "" }

# Generate the fixture files for `TENDERMINT_RPC_URL` using the prover parameter.
[group('generate')]
generate-fixtures-sp1-ics07: clean-foundry install-operator install-relayer
  @echo "Generating fixtures... This may take a while (up to 20 minutes)"
  TENDERMINT_RPC_URL="${TENDERMINT_RPC_URL%/}" && \
  CURRENT_HEIGHT=$(curl "$TENDERMINT_RPC_URL"/block | jq -r ".result.block.header.height") && \
  TRUSTED_HEIGHT=$(($CURRENT_HEIGHT-100)) && \
  TARGET_HEIGHT=$(($CURRENT_HEIGHT-10)) && \
  echo "For tendermint fixtures, trusted block: $TRUSTED_HEIGHT, target block: $TARGET_HEIGHT, from $TENDERMINT_RPC_URL" && \
  parallel --progress --shebang --ungroup -j 6 ::: \
    "RUST_LOG=info SP1_PROVER=network operator fixtures update-client --trusted-block $TRUSTED_HEIGHT --target-block $TARGET_HEIGHT -o 'test/sp1-ics07/fixtures/update_client_fixture-plonk.json' {{private_cluster}}" \
    "sleep 20 && RUST_LOG=info SP1_PROVER=network operator fixtures update-client --trusted-block $TRUSTED_HEIGHT --target-block $TARGET_HEIGHT -p groth16 -o 'test/sp1-ics07/fixtures/update_client_fixture-groth16.json' {{private_cluster}}" \
    "sleep 40 && RUST_LOG=info SP1_PROVER=network operator fixtures update-client-and-membership --key-paths clients/07-tendermint-0/clientState,clients/07-tendermint-001/clientState --trusted-block $TRUSTED_HEIGHT --target-block $TARGET_HEIGHT -o 'test/sp1-ics07/fixtures/uc_and_memberships_fixture-plonk.json' {{private_cluster}}" \
    "sleep 60 && RUST_LOG=info SP1_PROVER=network operator fixtures update-client-and-membership --key-paths clients/07-tendermint-0/clientState,clients/07-tendermint-001/clientState --trusted-block $TRUSTED_HEIGHT --target-block $TARGET_HEIGHT -p groth16 -o 'test/sp1-ics07/fixtures/uc_and_memberships_fixture-groth16.json' {{private_cluster}}" \
    "sleep 80 && RUST_LOG=info SP1_PROVER=network operator fixtures membership --key-paths clients/07-tendermint-0/clientState,clients/07-tendermint-001/clientState --trusted-block $TRUSTED_HEIGHT -o 'test/sp1-ics07/fixtures/memberships_fixture-plonk.json' {{private_cluster}}" \
    "sleep 100 && RUST_LOG=info SP1_PROVER=network operator fixtures membership --key-paths clients/07-tendermint-0/clientState,clients/07-tendermint-001/clientState --trusted-block $TRUSTED_HEIGHT -p groth16 -o 'test/sp1-ics07/fixtures/memberships_fixture-groth16.json' {{private_cluster}}"
  cd e2e/interchaintestv8 && RUST_LOG=info SP1_PROVER=network GENERATE_SOLIDITY_FIXTURES=true E2E_PROOF_TYPE=plonk go test -v -run '^TestWithSP1ICS07TendermintTestSuite/Test_DoubleSignMisbehaviour$' -timeout 40m
  cd e2e/interchaintestv8 && RUST_LOG=info SP1_PROVER=network GENERATE_SOLIDITY_FIXTURES=true E2E_PROOF_TYPE=groth16 go test -v -run '^TestWithSP1ICS07TendermintTestSuite/Test_BreakingTimeMonotonicityMisbehaviour' -timeout 40m
  cd e2e/interchaintestv8 && RUST_LOG=info SP1_PROVER=network GENERATE_SOLIDITY_FIXTURES=true E2E_PROOF_TYPE=groth16 go test -v -run '^TestWithSP1ICS07TendermintTestSuite/Test_100_Membership' -timeout 40m
  cd e2e/interchaintestv8 && RUST_LOG=info SP1_PROVER=network GENERATE_SOLIDITY_FIXTURES=true E2E_PROOF_TYPE=plonk go test -v -run '^TestWithSP1ICS07TendermintTestSuite/Test_25_Membership' -timeout 40m
  @echo "Fixtures generated at 'test/sp1-ics07/fixtures'"

# Generate the code from pritibuf using `buf generate`. (Only used for relayer testing at the moment)
[group('generate')]
generate-buf:
    @echo "Generating Protobuf files for relayer"
    buf generate --template buf.gen.yaml

shadowfork := if env("ETH_RPC_URL", "") == "" { "--no-match-path test/shadowfork/*" } else { "" }

# Run all the foundry tests
[group('test')]
test-foundry testname=".\\*":
	forge test -vvv --show-progress --fuzz-runs 5000 --match-test ^{{testname}}\(.\*\)\$ {{shadowfork}}
	@ {{ if shadowfork == "" { "" } else { 'echo ' + BOLD + YELLOW + 'Ran without shadowfork tests since ETH_RPC_URL was not set' } }}

# Run the benchmark tests
[group('test')]
test-benchmark testname=".\\*":
	forge test -vvv --show-progress --gas-report --match-path test/solidity-ibc/BenchmarkTest.t.sol --match-test {{testname}}

# Run the cargo tests
[group('test')]
test-cargo testname="--all":
	cargo test {{testname}} --locked --no-fail-fast -- --nocapture

# Run the tests in abigen
[group('test')]
test-abigen:
	@echo "Running abigen tests..."
	cd packages/go-abigen && go test -v ./...

# Run any e2e test using the test's full name. For example, `just test-e2e TestWithIbcEurekaTestSuite/Test_Deploy`
[group('test')]
test-e2e testname: clean-foundry install-relayer
	@echo "Running {{testname}} test..."
	cd e2e/interchaintestv8 && go test -v -run '^{{testname}}$' -timeout 120m

# Run any e2e test in the IbcEurekaTestSuite. For example, `just test-e2e-eureka Test_Deploy`
[group('test')]
test-e2e-eureka testname:
	@echo "Running {{testname}} test..."
	just test-e2e TestWithIbcEurekaTestSuite/{{testname}}

# Run any e2e test in the RelayerTestSuite. For example, `just test-e2e-relayer Test_RelayerInfo`
[group('test')]
test-e2e-relayer testname:
	@echo "Running {{testname}} test..."
	just test-e2e TestWithRelayerTestSuite/{{testname}}

# Run any e2e test in the CosmosRelayerTestSuite. For example, `just test-e2e-cosmos-relayer Test_RelayerInfo`
[group('test')]
test-e2e-cosmos-relayer testname:
	@echo "Running {{testname}} test..."
	just test-e2e TestWithCosmosRelayerTestSuite/{{testname}}

# Run anu e2e test in the SP1ICS07TendermintTestSuite. For example, `just test-e2e-sp1-ics07 Test_Deploy`
[group('test')]
test-e2e-sp1-ics07 testname: install-operator
	@echo "Running {{testname}} test..."
	just test-e2e TestWithSP1ICS07TendermintTestSuite/{{testname}}

# Run any e2e test in the MultichainTestSuite. For example, `just test-e2e-multichain Test_Deploy`
[group('test')]
test-e2e-multichain testname:
	@echo "Running {{testname}} test..."
	just test-e2e TestWithMultichainTestSuite/{{testname}}

# Run the e2e tests in the IbcEurekaSolanaTestSuite. For example, `just test-e2e-solana Test_Deploy`
[group('test')]
test-e2e-solana testname:
	@echo "Running {{testname}} test..."
	just test-e2e TestWithIbcEurekaSolanaTestSuite/{{testname}}

# Run the Solana Anchor e2e tests
[group('test')]
test-anchor-solana *ARGS:
	@echo "Running Solana Client Anchor tests..."
	@echo "🦀 Using {{anchor_cmd}}"
	(cd programs/solana && {{anchor_cmd}} test {{ARGS}})

# Run Solana unit tests (mollusk + litesvm)
[group('test')]
test-solana *ARGS:
	@echo "Building and running Solana unit tests..."
	@echo "🦀 Using {{anchor_cmd}}"
	@if [ "{{anchor_cmd}}" = "anchor-nix" ]; then \
		(cd programs/solana && anchor-nix unit-test {{ARGS}}); \
	else \
		(cd programs/solana && anchor build) && \
		echo "✅ Build successful, running cargo tests" && \
		(cd programs/solana && cargo test {{ARGS}}); \
	fi

# Clean up the foundry cache and out directories
[group('clean')]
clean-foundry:
	@echo "Cleaning up cache and out directories"
	-rm -rf cache out broadcast # ignore errors

# Clean up the cargo artifacts using `cargo clean`
[group('clean')]
clean-cargo:
	@echo "Cleaning up cargo target directory"
	cargo clean
	cd programs/sp1-programs && cargo clean

# Run Slither static analysis on contracts
[group('security')]
slither:
	@echo "Running Slither static analysis..."
	slither . --config-file .slither.config.json
