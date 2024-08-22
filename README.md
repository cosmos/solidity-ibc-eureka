# IBC Eureka in Solidity  [![Github Actions][gha-badge]][gha] [![Foundry][foundry-badge]][foundry] [![License: MIT][license-badge]][license]

[gha]: https://github.com/srdtrk/solidity-ibc-eureka/actions
[gha-badge]: https://github.com/srdtrk/solidity-ibc-eureka/actions/workflows/e2e.yml/badge.svg
[foundry]: https://getfoundry.sh/
[foundry-badge]: https://img.shields.io/badge/Built%20with-Foundry-FFDB1C.svg
[license]: https://opensource.org/licenses/MIT
[license-badge]: https://img.shields.io/badge/License-MIT-blue.svg

This is a work-in-progress IBC Eureka implementation in Solidity. IBC Eureka is a simplified version of the IBC protocol that is encoding agnostic.

## Overview

`solidity-ibc-eureka` is an implementation of IBC in Solidity.

### Project Structure

This project is structered as a [foundry](https://getfoundry.sh/) project with the following directories:

- `src/`: Contains the Solidity contracts.
- `test/`: Contains the Solidity tests.
- `scripts/`: Contains the Solidity scripts.
- `abi/`: Contains the ABIs of the contracts needed for end-to-end tests.
- `e2e/`: Contains the end-to-end tests, powered by [interchaintest](https://github.com/strangelove-ventures/interchaintest).

### Contracts

| **Contracts** | **Description** | **Status** |
|:---:|:---:|:---:|
| `ICS26Router.sol` | IBC Eureka router handles sequencing, replay protection, and timeout checks. Passes proofs to `ICS02Client.sol` for verification, and resolves `portId` for app callbacks. Provable IBC storage is stored in this contract.  | ✅ |
| `ICS02Client.sol` | IBC Eureka light client router resolves `clientId` for proof verification. It also stores the counterparty information for each client. | ✅ |
| `SdkICS20Transfer.sol` | IBC Eureka transfer application to send and receive tokens to/from `CosmosSDK`. | ✅ |
| `ICS27Controller.sol` | IBC Eureka interchain accounts controller. | ❌ |
| `ICS27Host.sol` | IBC Eureka interchain accounts host. | ❌ |

## Requirements

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
You can also run the tests with a "mock" prover that doesn't actually generate proofs and will accept any proof.

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

The contracts in this repository are benchmarked end-to-end using foundry. The following benchmarks were ran with the underlying [sp1-ics07-tendermint](https://github.com/cosmos/sp1-ics07-tendermint). About ~320,000 gas is used for each light client verification, and this is included in the gas costs below for `recvPacket`, `timeoutPacket` and `ackPacket`. At the time of writing, proof generation takes around 3 minutes 30 seconds. More granular and in-depth benchmarks are planned for the future.

| **Contract** | **Method** | **Description** | **Gas** |
|:---:|:---:|:---:|:---:|
| `SdkICS20Transfer.sol` | `sendTransfer` | Initiating an IBC transfer with an `ERC20`. | 241,674 |
| `ICS26Router.sol` | `recvPacket` | Receiving _back_ an `ERC20` token. | 620,758 |
| `ICS26Router.sol` | `recvPacket` | Receiving a _new_ Cosmos token for the first time. (Deploying an `ERC20` contract) | 1,521,712 |
| `ICS26Router.sol` | `ackPacket` | Acknowledging an ICS20 packet. | 508,261 |
| `ICS26Router.sol` | `timeoutPacket` | Timing out an ICS20 packet | 555,121 |

## License

This project is licensed under MIT.

## Acknowledgements

This project was bootstrapped with this [template](https://github.com/PaulRBerg/foundry-template). Implementations of IBC specifications in [solidity](https://github.com/hyperledger-labs/yui-ibc-solidity/), [CosmWasm](https://github.com/srdtrk/cw-ibc-lite), [golang](https://github.com/cosmos/ibc-go), and [rust](https://github.com/cosmos/ibc-rs) were used as references.
