---
title: IBC Eureka CLI
id: eureka-cli
---

# IBC Eureka CLI

IBC Eureka is the official native implementation of IBC v2.

The IBC Eureka CLI is a tool for interacting with IBC Eureka, enabling trust-minimized token transfers between Ethereum and Cosmos chains.

This tool is currently in `devnet` phase.

## Prerequisites

- Go 1.19 or later
- Access to Ethereum and Cosmos endpoints
- Private keys for both chains
- Sepolia ETH for Ethereum gas fees on the Sepolia Testnet
- An ERC20 token on Sepolia Ethereum such as our [$TNERC](https://sepolia.etherscan.io/token/0xa4ff49eb6e2ea77d7d8091f1501385078642603f)

## Installation

1. First clone the repository
```bash
git clone https://github.com/cosmos/solidity-ibc-eureka
```

2. Navigate to where the repo now lives on your machine, and move into the cmd folder to build the CLI

```bash
cd e2e/interchaintestv8/cmd
```

3. Build the CLI tool to test transferring and relaying Eureka packets on devnet
```bash
go build -o eureka-cli
```

## Keys

Once the CLI is built, the next steps are to set up the Ethereum Sepolia and Cosmos devnet keys and addresses to use. Currently, you'll need to store your private keys for both Cosmos and Ethereum as environment variables.

  - > **warning:** Do not use mainnet keys for this testing CLI.

There are three accounts required:
1. `ETH_PRIVATE_KEY:` You can retrieve an Ethereum private key from within Metamask by creating a new account > navigating to "Account details" > and pressing "Show private key"
    - To test a transfer from Ethereum Sepolia to the Cosmos Devnet, you'll need to have testnet ETH on this account. You can use any Ethereum Sepolia faucet for this, an example being: https://cloud.google.com/application/web3/faucet/ethereum/sepolia
    - You'll also be transferring an ERC20 token. you can use https://tokentool.bitbond.com/create-token/erc20-token/ethereum-sepolia to create a new ERC20 token on Ethereum Sepolia Testnet and use that in your command to do a Eureka transfer from Sepolia Testnet to Cosmos Devnet.
2. `COSMOS_PRIVATE_KEY:` This will be used as the receiver of an Ethereum Sepolia to Cosmos Devnet transfer, and the initiator of a transfer in the other direction. You can retrieve a Cosmos `unarmored-hex` private key by following the following steps:
    1. Installing a node daemon CLI: `simd` or <code><a href="https://github.com/cosmos/gaia">gaiad</a></code>.
    2. Adding keys to the daemon CLI: `gaiad keys add <account-name> --recover`
    3. Entering the BIP-39 mnemonic for the account you want to add. (Remove `--recover` to generate new)
    4. Exporting the unarmored hex: `gaiad keys export <account-name> --unarmored-hex --unsafe`
3. `RELAYER_WALLET:` For devnet, we are providing relayer keys manually, reach out to the ICL team and we will provide the private key for your use. 

Note: All three of the above are private keys, hexadecimal, 64 characters long.

Once all the necessary private keys are obtained, run the following command to set them as environment variables:

```bash
export ETH_PRIVATE_KEY="your-ethereum-private-key"
export COSMOS_PRIVATE_KEY="your-cosmos-unarmored-hex-private-key"
export RELAYER_WALLET="ask-icl-team-for-the-testing-key"
```

## Basic Commands

### Transfer ERC20 Tokens from Ethereum to Cosmos

Format:

```bash
go run ./ transfer-from-eth-to-cosmos [amount] [erc20-contract-address] [to-address] [flags]
```

Example:

```bash
go run ./ transfer-from-eth-to-cosmos 1 0xA4ff49eb6E2Ea77d7D8091f1501385078642603f cosmos1u5d4hk8294fs9pq556jxmlju2ceh4jmurcpfv7 \
  --eth-rpc="https://ethereum-sepolia-rpc.publicnode.com" \
  --ics20-address="0xbb87C1ACc6306ad2233a4c7BBE75a1230409b358" \
  --source-client-id="client-0"
```

This will give you a `tx hash` in the output, needed for relaying.

### Relay the Transaction from Ethereum to Cosmos

Format:

```bash
go run ./ relay_tx [txHash] [flags]
```

Example:

```bash
go run ./ relay_tx 0xed13b2567a00eae7d0a6c8e24d1cf6342116d1d89d72ff9b52b690cdd3a5dd98 \
  --eth-rpc="https://ethereum-sepolia-rpc.publicnode.com" \
  --cosmos-rpc="https://eureka-devnet-node-01-rpc.dev.skip.build:443"
```

### Check Balance of ETH Account

To check the balance on Ethereum, enter an Ethereum `0x` address into the `[address]` flag.

```bash
# Usage:
go run ./ balance [address] [optional-denom-or-erc20-address] [flags]
```

Example:

```bash
go run ./ balance 0x94B00F484232D55Cc892BbE0b0C1c4a9ad112098
```

Output:

```bash
0xA4ff49eb6E2Ea77d7D8091f1501385078642603f: 999999997
ETH: 0.092298623946995983
```

### Check Balance of Cosmos Account

To check the balance on Cosmos, enter a Cosmos `cosmos1` address into the `[address]` flag.

```bash
#Usage:
go run ./ balance [address] [optional-denom-or-erc20-address] [flags]
```

Example:

```bash
go run ./ balance cosmos1u5d4hk8294fs9pq556jxmlju2ceh4jmurcpfv7
```

Output:

```bash
IBC Denom: ibc/2351096B1729B2C64AED9F6AFD4A4BC28EB56F624881556947A8C48EDB9ED444
transfer/08-wasm-0/0xa4ff49eb6e2ea77d7d8091f1501385078642603f: 1
```

You can make an equivalent query using `gaiad` with:

```bash
gaiad query bank balances cosmos1u5d4hk8294fs9pq556jxmlju2ceh4jmurcpfv7 --chain-id=highway-dev-1 --node=https://eureka-devnet-node-01-rpc.dev.skip.build:443 --output json
```

### Transfer Tokens from Ethereum to Cosmos (Back Again!)

You'll need the IBC Denom (from the `balance` command above) to send from Cosmos back to Ethereum.

Format:

```bash
go run ./ transfer-from-cosmos-to-eth [amount] [denom] [to-ethereum-address] [flags]
```

Example:

```bash
go run ./ transfer-from-cosmos-to-eth 1 ibc/2351096B1729B2C64AED9F6AFD4A4BC28EB56F624881556947A8C48EDB9ED444 0x94B00F484232D55Cc892BbE0b0C1c4a9ad112098
```

This will give you a `tx hash` in the output, needed for relaying.

### Relay the Transaction from Cosmos to Ethereum

Format:

```bash
go run ./ relay_tx [txHash] [flags]
```

Example:

```bash
go run ./ relay_tx 28D0B356557DC625D62649E7B1E05B8730898389B8D888E9C920BED33429D9EB \
  --eth-rpc="https://ethereum-sepolia-rpc.publicnode.com" \
  --cosmos-rpc="https://eureka-devnet-node-01-rpc.dev.skip.build:443"
  ```

This is the same as the previous relay, but from the format of the `txHash`, the relayer knows in which direction the relay needs to happen.

Now you can run the `balance` commands again!

---

Please reach out to the Interchain Labs team if you have any issues!
