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
- [sp1-ics07-tendermint](https://github.com/cosmos/sp1-ics07-tendermint) (for end-to-end tests)

Foundry typically uses git submodules to manage contract dependencies, but this repository uses Node.js packages (via Bun) because submodules don't scale. You can install the contracts dependencies by running the following command:

```sh
bun install
```

You also need to have the `sp1-ics07-tendermint` operator binary installed on your machine to run the end-to-end tests. You can install it by running the following command:

```sh
just install-operator
```

## End to End Testing

There are several end-to-end tests in the `e2e/interchaintestv8` directory. These tests are written in Go and use the [`interchaintest`](https://github.com/strangelove-ventures/interchaintest) library. It spins up a local Ethereum and a Tendermint network and runs the tests found in [`e2e/interchaintestv8/ibc_eureka_test.go`](e2e/interchaintestv8/sp1_ics07_test.go). Some of the tests use the prover network to generate the proofs, so you need to provide your SP1 network private key to `.env` for these tests to pass.

> [!NOTE]
> If you are running on a Mac with an M chip, you will need to do the following:
> - Set up Rosetta
> - Enable Rosetta for Docker (in Docker Desktop: Settings -> General -> enable "Use Rosetta for x86_64/amd64 emulation on Apple Silicon")
> - Pull the foundry image with the following command:
> 
>     ```sh
>     docker pull --platform=linux/amd64 ghcr.io/foundry-rs/foundry:latest
>     ```

To run the tests, run the following command:

```sh
just test-e2e $TEST_NAME
```

Where `$TEST_NAME` is the name of the test you want to run, for example:

```sh
just test-e2e TestDeploy
```

## License

This project is licensed under MIT.

## Acknowledgements

This project was bootstrapped with this [template](https://github.com/PaulRBerg/foundry-template). Implementations of IBC specifications in [solidity](https://github.com/hyperledger-labs/yui-ibc-solidity/), [CosmWasm](https://github.com/srdtrk/cw-ibc-lite), [golang](https://github.com/cosmos/ibc-go), and [rust](https://github.com/cosmos/ibc-rs) were used as references.
