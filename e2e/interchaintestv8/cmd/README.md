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

```bash
# Clone the repository
git clone https://github.com/cosmos/solidity-ibc-eureka
```

```bash
cd solidity-ibc-eureka/e2e/interchaintestv8
# Build the CLI
go build -o eureka-cli
```

## Keys

Currently, you'll need to store your private keys for both Cosmos and Ethereum as environment variables.

>:warning: Do not use mainnet keys for this testing CLI.

```bash
export ETH_PRIVATE_KEY="your-ethereum-private-key"
export COSMOS_PRIVATE_KEY="your-cosmos-unarmored-hex-private-key"
export RELAYER_WALLET="ask-icl-team-for-the-testing-key"
```

All three of the above are private keys, hexadecimal, 64 characters long.

**You can retrieve an Ethereum private key from within Metamask.**

**You can retrieve a Cosmos `unarmored-hex` private key by:**

1. Installing a node daemon CLI: `simd` or <code><a href="https://github.com/cosmos/gaia">gaiad</a></code>.
2. Adding keys to the daemon CLI: `gaiad keys add <account-name> --recover`
3. Entering the BIP-39 mnemonic for the account you want to add. (Remove `--recover` to generate new)
4. Exporting the unarmored hex: `gaiad keys export <account-name> --unarmored-hex --unsafe`

**:information_source: For devnet, we are providing relayer keys manually, reach out to the ICL team for one**

## Basic Commands

### Transfer ERC20 Tokens from Ethereum to Cosmos

```bash
# Usage:
eureka-cli transfer-from-eth-to-cosmos [amount] [erc20-contract-address] [to-address] [flags]
```

Example:

```bash
go run ./ transfer-from-eth-to-cosmos 1 0xA4ff49eb6E2Ea77d7D8091f1501385078642603f 0xAe3E5CCaF3216de61090E68Cf5a191f3b75CaAd3 \
  --eth-rpc="https://ethereum-sepolia-rpc.publicnode.com" \
  --ics20-address="0xbb87C1ACc6306ad2233a4c7BBE75a1230409b358" \
  --source-client-id="client-0"
```

This will give you a `tx hash` in the output.

### Relay the Transaction

```bash
# Usage:
eureka-cli relay_tx [txHash] [flags]
```

Example:

```bash
go run ./ relay_tx 0xed13b2567a00eae7d0a6c8e24d1cf6342116d1d89d72ff9b52b690cdd3a5dd98 \
  --eth-rpc="https://ethereum-sepolia-rpc.publicnode.com" \
  --cosmos-rpc="https://eureka-devnet-node-01-rpc.dev.skip.build:443" \
  --verbose
```

### Check Balance of ETH Account

```bash
# Usage:
eureka-cli balance [address] [optional-denom-or-erc20-address] [flags]
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
