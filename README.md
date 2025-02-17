# IBC Eureka in Solidity  [![Github Actions][gha-badge]][gha] [![Foundry][foundry-badge]][foundry] [![License: MIT][license-badge]][license] [![Code Coverage][codecov-badge]][codecov]

[gha]: https://github.com/srdtrk/solidity-ibc-eureka/actions
[gha-badge]: https://github.com/srdtrk/solidity-ibc-eureka/actions/workflows/e2e.yml/badge.svg
[foundry]: https://getfoundry.sh/
[foundry-badge]: https://img.shields.io/badge/Built%20with-Foundry-FFDB1C.svg
[license]: https://opensource.org/licenses/MIT
[license-badge]: https://img.shields.io/badge/License-MIT-blue.svg
[codecov]: https://codecov.io/github/cosmos/solidity-ibc-eureka
[codecov-badge]: https://codecov.io/github/cosmos/solidity-ibc-eureka/graph/badge.svg?token=lhplGORQxX

This is a work-in-progress IBC Eureka implementation in Solidity. IBC Eureka is a simplified version of the IBC protocol that is encoding agnostic. This enables a trust-minimized IBC connection between ethereum and a Cosmos SDK chain.

## Overview

`solidity-ibc-eureka` is an implementation of IBC in Solidity.

### Project Structure

This project is structured as a [foundry](https://getfoundry.sh/) project with the following directories:

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
    - `cw-ics08-wasm-eth/`: Contains the (WIP) CosmWasm 08-wasm light client for Ethereum
- `packages/`: Contains the Rust packages for the project.

### Contracts

| **Contracts** | **Description** | **Status** |
|:---:|:---:|:---:|
| `ICS26Router.sol` | IBC Eureka router handles sequencing, replay protection, and timeout checks. Passes proofs to light clients for verification, and resolves `portId` for app callbacks. Provable IBC storage is stored in this contract.  | ✅ |
| `ICS20Transfer.sol` | IBC Eureka transfer application to send and receive tokens to/from another Eureka transfer implementation. | ✅ |
| `SP1ICS07Tendermint.sol` | The light client contract, and the entry point for SP1 proofs. | ✅ |
| `ICS27Controller.sol` | IBC Eureka interchain accounts controller. | ❌ |
| `ICS27Host.sol` | IBC Eureka interchain accounts host. | ❌ |

### SP1 Programs for the Light Client

|     **Programs**    |                                                                                                                                     **Description**                                                                                                                                     | **Status** |
|:-------------------:|:---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------:|:----------:|
|   `update-client`   | Once the initial client state and consensus state are submitted, future consensus states can be added to the client by submitting IBC Headers. These headers contain all necessary information to run the Comet BFT Light Client protocol. Also supports partial misbehavior check.     |      ✅     |
|     `membership`    | As consensus states are added to the client, they can be used for proof verification by relayers wishing to prove packet flow messages against a particular height on the counterparty. This uses the `verify_membership` and `verify_non_membership` methods on the tendermint client. |      ✅     |
| `uc-and-membership` | This is a program that combines `update-client` and `membership` to update the client, and prove membership of packet flow messages against the new consensus state.                                                                                                                    |      ✅     |
|    `misbehaviour`   | In case, the malicious subset of the validators exceeds the trust level of the client; then the client can be deceived into accepting invalid blocks and the connection is no longer secure. The tendermint client has some mitigations in place to prevent this.                       |      ✅     |


## Requirements

- [Rust](https://rustup.rs/)
- [Foundry](https://book.getfoundry.sh/getting-started/installation)
- [Bun](https://bun.sh/)
- [Just](https://just.systems/man/en/)
- [SP1](https://succinctlabs.github.io/sp1/getting-started/install.html) (for end-to-end tests)

Foundry typically uses git submodules to manage contract dependencies, but this repository uses Node.js packages (via Bun) because submodules don't scale. You can install the contracts dependencies by running the following command:

```sh
bun install
```

You also need to have the operator and relayer binaries installed on your machine to run some of the end-to-end tests. You can install them by running the following commands:

```sh
just install-operator
just install-relayer
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

There are three test suites in the `e2e/interchaintestv8` directory:

- `TestWithIbcEurekaTestSuite`: This test suite tests the IBC Eureka contracts via manual relaying (requires the operator to be installed).
    - To run any of the tests, run the following command:
        ```sh
        just test-e2e $TEST_NAME
        ```
- `TestWithRelayerTestSuite`: This test suite tests the IBC Eureka contracts via the relayer (requires the relayer and operator to be installed).
    - To run any of the tests, run the following command:
        ```sh
        just test-e2e-relayer $TEST_NAME
        ```
- `TestWithSP1ICS07TendermintTestSuite`: This test suite tests the SP1 ICS07 Tendermint light client (requires the operator to be installed).
    - To run any of the tests, run the following command:
        ```sh
        just test-e2e-sp1-ics07 $TEST_NAME
        ```

Where `$TEST_NAME` is the name of the test you want to run, for example:

```sh
just test-e2e TestDeploy_Groth16
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
| `ICS26Router.sol` | `sendPacket` | Initiating an IBC transfer with an `ERC20`. | ~149,000 | ~149,000 |
| `ICS26Router.sol` | `recvPacket` | Receiving _back_ an `ERC20` token. | ~522,788 | ~605,734 |
| `ICS26Router.sol` | `recvPacket` | Receiving a _new_ Cosmos token for the first time. (Deploying an `ERC20` contract) | ~1,092,349 | ~1,176,657 |
| `ICS26Router.sol` | `ackPacket` | Acknowledging an ICS20 packet. | ~391,990 | ~475,788 |
| `ICS26Router.sol` | `timeoutPacket` | Timing out an ICS20 packet | ~464,141 | ~548,602 |

### Aggregated Packet Benchmarks

The gas costs are substantially lower when aggregating multiple packets into a single proof, as long as the packets are submitted in the same tx.
Since there is no meaningful difference in gas costs between plonk and groth16 in the aggregated case, they are not separated in the table below.

| **ICS26Router Method** | **Description** | **Avg Gas (25 packets)** | **Avg Gas (50 packets)** |
|:---:|:---:|:---:|:---:|
| `multicall/recvPacket` | Receiving _back_ an `ERC20` token. | ~183,517 | ~176,964 |
| `multicall/ackPacket` | Acknowledging an ICS20 packet. | ~90,903 | ~85,198 |

Note: These gas benchmarks are with Groth16.

## Run ICS-07 Tendermint Light Client End to End

1. Set the environment variables by filling in the `.env` file with the following:

    ```sh
    cp .env.example .env
    ```

    You need to fill in the `PRIVATE_KEY`, `SP1_PROVER`, `TENDERMINT_RPC_URL`, and `RPC_URL`. You also need the `NETWORK_PRIVATE_KEY` field if you are using the SP1 prover network.

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
    If you run the operator with the `network` prover, you need to provide your SP1 network private key with `NETWORK_PRIVATE_KEY=0xyourprivatekey` in `.env`.

    ```sh
    RUST_LOG=info cargo run --bin operator --release -- start
    ```

## Etheruem Light Client

> [!CAUTION]
> ⚠ The Ethereum Light Client is currently under heavy development, and is not ready for use.

This repository contains an Ethereum light client which is implemented as two separate layers:

- A CosmWasm contract that supports the 08-wasm light client interface in `programs/cw-ics08-wasm-eth`
- A stateless light client verification package in `packages/ethereum-light-client`

## Security Assumtions

IBC is a peer-to-peer light client based interop protocol. As such, this repository contains two light clients:

- The SP1 light client to verify the consensus state of the Cosmos SDK chain.
- The Ethereum light client to verify the consensus state of the Ethereum chain.

In IBC, the security of the connection is based on the security of the light clients and the validators of the chains. However, IBC light clients also define conditions under which they may become frozen, for example, if two valid and conflicting headers are submitted, or if the light client is not updated for a long time. In this case, Cosmos SDK chains rely on governance to resolve the issue by restarting the light client. However, Ethereum does not have a governance mechanism to restart the light client. **As such, the Solidity implementation of IBC requires a (timelocked) security council to restart the light client in case of a freeze.**

This security council also has the power to upgrade all IBC contracts in case of a vulnerability, or to add new features. This is a trade-off between decentralization and security, and is a necessary step to ensure the security of the IBC connection. Ideally, the security council should be equal to the validators of the counterparty Cosmos SDK chain for the sake of decentralization. And since IBC is a general-purpose protocol, IBC itself can be used to allow the governance of the Cosmos SDK chain to make calls to the Ethereum contracts to upgrade them. This is a future feature, called `govAdmin`, that is not yet implemented. (#278)

### Security Council and the Governance Admin

Although the governance admin is not yet implemented, the contract for tracking both admins is in [`IBCUUPSUpgradeable.sol`](./contracts/utils/IBCUUPSUpgradeable.sol). This contract is used in (i.e. inherited by) [`ICS26Router.sol`](./contracts/ICS26Router.sol), which stores the security council (referred to as `timelockedAdmin`) and the governance admin (referred to as `govAdmin`). Other IBC contracts which require admin access, or upgradability, should simply query the `ICS26Router` contract for the current admin ([see how it is done in `ICS20Transfer.sol`](https://github.com/cosmos/solidity-ibc-eureka/blob/1db4d38d00f7935e2aa4564b7026182a4c095ef1/contracts/ICS20Transfer.sol#L487-L492)).

The governance admin field can be set by the security council once the feature is implemented. Until then, the security council is the only admin. By the `IBCUUPSUpgradeable` contract, the security council and the governance admin have equal powers:

- Either admin can set the other admin.
- Either admin can manage roles on IBC contracts.
- Either admin can upgrade the IBC contracts.

The only difference between the two admins is that the security council should apply itself a timelock before setting the governance admim. This is to ensure that once a governance admin is set, the security council only has any power if IBC light clients are frozen (i.e. the governance admin is also frozen).

> [!WARNING]
> The timelock on the security council is not enforced in the IBC contracts, but should be enforced by the security council itself.
> The timelock on the security council should be at least as long as the timelock on the governance admin (if any) + the time it takes for governance proposals to pass.

### Roles and Permissions

The IBC contracts use `AccessControl` to manage roles and permissions and allow the admins to reassign roles. The roles are:

| **Role Name** | **Contract** | **Default** | **Description** |
|:---:|:---:|:---:|:---:|
| `PAUSER_ROLE` | `ICS20Transfer.sol` | Set at initialization. | Bearer can (un)pause the contract. |
| `RATE_LIMITER_ROLE` | `Escrow.sol` | `address(0)` | Bearer can set withdraw rate limits per `ERC20`. |
| `LIGHT_CLIENT_MIGRATOR_ROLE_{client_id}` | `ICS26Router.sol` | Creator of the light client. | Bearer can migrate the light client with `client_id`. |

## License

This project is licensed under MIT.

## Acknowledgements

This project was bootstrapped with this [template](https://github.com/PaulRBerg/foundry-template). Implementations of IBC specifications in [solidity](https://github.com/hyperledger-labs/yui-ibc-solidity/), [CosmWasm](https://github.com/srdtrk/cw-ibc-lite), [golang](https://github.com/cosmos/ibc-go), and [rust](https://github.com/cosmos/ibc-rs) were used as references. We are also grateful to [unionlabs](https://github.com/unionlabs/union/) for their `08-wasm` Ethereum light client implementation for ibc-go which our own implementation is based on.
