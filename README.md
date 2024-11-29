# IBC Eureka in Solidity  [![Github Actions][gha-badge]][gha] [![Foundry][foundry-badge]][foundry] [![License: MIT][license-badge]][license] [![Code Coverage][codecov-badge]][codecov]

[gha]: https://github.com/srdtrk/solidity-ibc-eureka/actions
[gha-badge]: https://github.com/srdtrk/solidity-ibc-eureka/actions/workflows/e2e.yml/badge.svg
[foundry]: https://getfoundry.sh/
[foundry-badge]: https://img.shields.io/badge/Built%20with-Foundry-FFDB1C.svg
[license]: https://opensource.org/licenses/MIT
[license-badge]: https://img.shields.io/badge/License-MIT-blue.svg
[codecov]: https://codecov.io/github/cosmos/solidity-ibc-eureka
[codecov-badge]: https://codecov.io/github/cosmos/solidity-ibc-eureka/graph/badge.svg?token=lhplGORQxX

This is a work-in-progress IBC Eureka implementation in Solidity. IBC Eureka is a simplified version of the IBC protocol that is encoding agnostic. This project also includes an [SP1](https://github.com/succinctlabs/sp1) based tendermint light client for the Ethereum chain, and a POC relayer implementation.

## Overview

`solidity-ibc-eureka` is an implementation of IBC in Solidity.

### Project Structure

This project is structered as a [foundry](https://getfoundry.sh/) project with the following directories:

- `contracts/`: Contains the Solidity contracts.
- `test/`: Contains the Solidity tests.
- `scripts/`: Contains the Solidity scripts.
- `abi/`: Contains the ABIs of the contracts needed for end-to-end tests.
- `abigen/`: Contains the abi generated go files for the Solidity contracts.
- `e2e/`: Contains the end-to-end tests, powered by [interchaintest](https://github.com/strangelove-ventures/interchaintest).
- `programs/`: Contains the Rust programs for the project.
    - `relayer/`: Contains the relayer implementation.
    - `operator/`: Contains the operator for the SP1 light client.
    - `sp1-programs/`: Contains the SP1 programs for the light client.
- `packages/`: Contains the Rust packages for the project.

### SP1 Programs for the Light Client

|     **Programs**    |                                                                                                                                     **Description**                                                                                                                                     | **Status** |
|:-------------------:|:---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------:|:----------:|
|   `update-client`   | Once the initial client state and consensus state are submitted, future consensus states can be added to the client by submitting IBC Headers. These headers contain all necessary information to run the Comet BFT Light Client protocol. Also supports partial misbehavior check.     |      ✅     |
|     `membership`    | As consensus states are added to the client, they can be used for proof verification by relayers wishing to prove packet flow messages against a particular height on the counterparty. This uses the `verify_membership` and `verify_non_membership` methods on the tendermint client. |      ✅     |
| `uc-and-membership` | This is a program that combines `update-client` and `membership` to update the client, and prove membership of packet flow messages against the new consensus state.                                                                                                                    |      ✅     |
|    `misbehaviour`   | In case, the malicious subset of the validators exceeds the trust level of the client; then the client can be deceived into accepting invalid blocks and the connection is no longer secure. The tendermint client has some mitigations in place to prevent this.                       |      ✅     |

### Contracts

| **Contracts** | **Description** | **Status** |
|:---:|:---:|:---:|
| `ICS26Router.sol` | IBC Eureka router handles sequencing, replay protection, and timeout checks. Passes proofs to `ICS02Client.sol` for verification, and resolves `portId` for app callbacks. Provable IBC storage is stored in this contract.  | ✅ |
| `ICS02Client.sol` | IBC Eureka light client router resolves `clientId` for proof verification. It also stores the counterparty information for each client. | ✅ |
| `ICS20Transfer.sol` | IBC Eureka transfer application to send and receive tokens to/from another Eureka transfer implementation. | ✅ |
| `SP1ICS07Tendermint.sol` | The light client contract, and the entry point for SP1 proofs. | ✅ |
| `ICS27Controller.sol` | IBC Eureka interchain accounts controller. | ❌ |
| `ICS27Host.sol` | IBC Eureka interchain accounts host. | ❌ |

## Requirements

- [Rust](https://rustup.rs/)
- [Foundry](https://book.getfoundry.sh/getting-started/installation)
- [Bun](https://bun.sh/)
- [Just](https://just.systems/man/en/)
- [SP1](https://succinctlabs.github.io/sp1/getting-started/install.html) (for end-to-end tests)
- [sp1-ics07-tendermint](https://github.com/cosmos/sp1-ics07-tendermint) (for end-to-end tests)

Foundry typically uses git submodules to manage contract dependencies, but this repository uses Node.js packages (via Bun) because submodules don't scale. You can install the contracts dependencies by running the following command:

```sh
bun install
```

You also need to have the `sp1-ics07-tendermint` operator binary installed on your machine to run the end-to-end tests. You can install it by running the following command:

```sh
just install-operator
```

> [!TIP]
> Nix users can enter a development shell with all the necessary dependencies by running:
> 
> ```sh
> nix develop
> ```

## Unit Testing

There are multiple unit tests for the solidity contracts located in the `test/` directory. The tests are written in Solidity using [foundry/forge](https://book.getfoundry.sh/forge/writing-tests).

To run all the tests, run the following command:

```sh
just test-foundry
```

The recipe also accepts a `testname` argument that will only run the test with the given name. For example:

```shell
just test-foundry test_success_sendTransfer
```

## End to End Testing

There are several end-to-end tests in the `e2e/interchaintestv8` directory. These tests are written in Go and use the [`interchaintest`](https://github.com/strangelove-ventures/interchaintest) library. 
It spins up a local Ethereum and a Tendermint network and runs the tests found in [`e2e/interchaintestv8/ibc_eureka_test.go`](e2e/interchaintestv8/ibc_eureka_test.go). 
Some of the tests use the prover network to generate the proofs, so you need to provide your SP1 network private key to `.env` for these tests to pass.

To prepare for running the e2e tests, you need to make sure you have done the following:
* Installed the `sp1-ics07-tendermint` operator binary (see instructions above)
* Set up an .env file (see the instructions in the `.env.example` file)
* If you have made changes to the contract interfaces or types, you need to update the ABIs by running `just generate-abi`

> [!NOTE]
> If you are running on a Mac with an M chip, you will need to do the following:
> - Set up Rosetta
> - Enable Rosetta for Docker (in Docker Desktop: Settings -> General -> enable "Use Rosetta for x86_64/amd64 emulation on Apple Silicon")
> - Pull the foundry image with the following command:
> 
>     ```sh
>     docker pull --platform=linux/amd64 ghcr.io/foundry-rs/foundry:latest
>     ```

### Running the tests

To run the tests, run the following command:

```sh
just test-e2e $TEST_NAME
```

Where `$TEST_NAME` is the name of the test you want to run, for example:

```sh
just test-e2e TestDeploy
```

## Linting

Before committing, you should lint your code to ensure it follows the style guide. You can do this by running the following command:

```sh
just lint
```

## End to End Benchmarks

The contracts in this repository are benchmarked end-to-end using foundry. The following benchmarks were ran with the underlying [sp1-ics07-tendermint](https://github.com/cosmos/sp1-ics07-tendermint). About ~230,000 gas is used for each light client verification (groth16), and this is included in the gas costs below for `recvPacket`, `timeoutPacket` and `ackPacket`. At the time of writing, proof generation takes around 1 minute. More granular and in-depth benchmarks are planned for the future.

### Single Packet Benchmarks

The following benchmarks are for a single packet transfer without aggregation.

| **Contract** | **Method** | **Description** | **Gas (groth16)** | **Gas (plonk)** |
|:---:|:---:|:---:|:---:|:---:|
| `ICS26Router.sol` | `sendPacket` | Initiating an IBC transfer with an `ERC20`. | ~205,000 | ~205,000 |
| `ICS26Router.sol` | `recvPacket` | Receiving _back_ an `ERC20` token. | ~484,000 | ~563,000 |
| `ICS26Router.sol` | `recvPacket` | Receiving a _new_ Cosmos token for the first time. (Deploying an `ERC20` contract) | ~1,422,000 | ~1,510,000 |
| `ICS26Router.sol` | `ackPacket` | Acknowledging an ICS20 packet. | ~370,000 | ~448,000 |
| `ICS26Router.sol` | `timeoutPacket` | Timing out an ICS20 packet | ~458,000 | ~545,000 |

### Aggregated Packet Benchmarks

The gas costs are substantially lower when aggregating multiple packets into a single proof, as long as the packets are submitted in the same tx.
Since there is no meaningful difference in gas costs between plonk and groth16 in the aggregated case, they are not separated in the table below.

| **ICS26Router Method** | **Description** | **Avg Gas (25 packets)** | **Avg Gas (50 packets)** |
|:---:|:---:|:---:|:---:|
| `multicall/recvPacket` | Receiving _back_ an `ERC20` token. | ~204,000 | ~197,000 |
| `multicall/ackPacket` | Acknowledging an ICS20 packet. | ~116,000 | ~110,000 |

Note: These gas benchmarks are with Groth16.

## Run ICS-07 Tendermint Light Client End to End

1. Set the environment variables by filling in the `.env` file with the following:

    ```sh
    cp .env.example .env
    ```

    You need to fill in the `PRIVATE_KEY`, `SP1_PROVER`, `TENDERMINT_RPC_URL`, and `RPC_URL`. You also need the `SP1_PRIVATE_KEY` field if you are using the SP1 prover network.

2. Deploy the `SP1ICS07Tendermint` contract:

    ```sh
    just deploy-sp1-ics07
    ```

    This will generate the `contracts/script/genesis.json` file which contains the initialization parameters for the contract. And then deploy the contract using `contracts/script/SP1ICS07Tendermint.s.sol`.
    If you see the following error, add `--legacy` to the command in the `justfile`:
    ```text
    Error: Failed to get EIP-1559 fees    
    ```

3. Your deployed contract address will be printed to the terminal.

    ```text
    == Return ==
    0: address <CONTRACT_ADDRESS>
    ```

    This will be used when you run the operator in step 5. So add this to your `.env` file.

    ```.env
    CONTRACT_ADDRESS=<CONTRACT_ADDRESS>
    ```

4. Run the Tendermint operator.

    To run the operator, you need to select the prover type for SP1. This is set in the `.env` file with the `SP1_PROVER` value (`network|local|mock`).
    If you run the operator with the `network` prover, you need to provide your SP1 network private key with `SP1_PRIVATE_KEY=0xyourprivatekey` in `.env`.

    ```sh
    RUST_LOG=info cargo run --bin operator --release -- start
    ```

## License

This project is licensed under MIT.

## Acknowledgements

This project was bootstrapped with this [template](https://github.com/PaulRBerg/foundry-template). Implementations of IBC specifications in [solidity](https://github.com/hyperledger-labs/yui-ibc-solidity/), [CosmWasm](https://github.com/srdtrk/cw-ibc-lite), [golang](https://github.com/cosmos/ibc-go), and [rust](https://github.com/cosmos/ibc-rs) were used as references. We are also grateful to [unionlabs](https://github.com/unionlabs/union/) for their `08-wasm` ethereum light client implementation for ibc-go.
