# IBC Eureka CLI

IBC Eureka is the official native implementation of IBC v2, enabling trust-minimized token transfers between Ethereum and Cosmos chains.

This tool is currently in **devnet** phase.

## Installation

Ensure you have:

- Go 1.19 or later installed
- Access to Ethereum and Cosmos endpoints
- Private keys for both chains

### **Build the CLI**

1. First clone the repository

```bash
git clone https://github.com/cosmos/solidity-ibc-eureka
```

2. Navigate to where the repo now lives on your machine, and move into the cmd folder to build the CLI

```bash
cd solidity-ibc-eureka/e2e/interchaintestv8/cmd
```

3. Fetch and checkout the `devenet-deployment` [sic] branch that contains the CLI tool:
```bash
git fetch
git checkout gjermund/devenet-deployment
```

4. Build the CLI tool to test transferring and relaying Eureka packets on devnet
```bash
go build -o eureka-cli
```

## **Usage & Documentation**

For **detailed setup, key management, and CLI commands**, refer to the **full documentation**:  
👉 **[Eureka Devnet Testing Guide](https://docs.skip.build/go/eureka/devnet-testing-guide)**  
