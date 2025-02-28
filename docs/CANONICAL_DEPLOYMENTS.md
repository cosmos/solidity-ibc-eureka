# Canonical Eureka deployments

## Purpose

We want to be able to reliably and repeatedly deploy Eureka contracts on many EVM supported networks.
We also need multiple deployments on multiple networks in order to be able to test end-to-end functionality.
All of this means that we need a deployment pipeline that can both deploy contracts on Mainnet and handle multiple deployments in Testnet.

## Deployment types

1. Testnet - staging
    - This is going to be secured with no timelock and contracts are going to be upgraded on every push to main.
2. Testnet - production
    - This is going to be the canonical deployment between EVM and the Hub. Contracts are going to be timelocked with a tiny delay and transactions will be signed using a Gnosis Safe to replicate mainnet conditions.
3. Mainnet - production
    - This is going to be the canonical deployment between ETH Mainnet (for now) and the Hub. Contracts are not going to be upgraded, instead deployed as new instances. Upgrades will have to be proposed manually to the Eureka Security Council Gnosis Safe.

## Implementation

There are two stages / types of deployments:
1. Deployment of contract instances
2. Upgrading the contract proxies to the new instances

### Deployment of contract instances

This assumes that we either haven't deployed any contracts on this network yet, or we want to deploy new ones. The procedure in either of these cases is similar.

The general procedure for deploying a contract will be:
1. Fetch dependency data if needed (e.g. Router address if deploying Transfer)
2. Deploy the contract onto the specified network
3. Check for contract deployment success
4. Update the mapping between address => deployment data (commit hash, initialization parameters)

### Upgrading contract proxies

We are going to reuse and verify the mapping data in order to upgrade a proxy:
1. Select the address we want to upgrade the proxy to
2. Verify that the on-chain data matches what we are seeing in the mapping
3. Depending on whether the network is mainnet or not:
   1. Testnet - automatically sign the transactions and upgrade the contract
   2. Dump the resulting transaction signature and parameters into an artifact

### Deployment schema

#### Folder structure
```
deployments/
├─ mainnet-prod/
│  ├─ <chain_id>.json
├─ testnet-staging/
│  ├─ <chain_id>.json
├─ testnet-prod/
│  ├─ <chain_id>.json
```

#### Deployment JSON schema
```json
{
  "contracts": {
    "ICS26Router": {
      "proxy": "<address>",
      "implementation": "<address>",
    },
    "ICS20Transfer": {
      "proxy": "<address>",
      "implementation": "<address>",
    },
    "Escrow": {
      "beacon": "<address>",
      "implementation": "<address>",
    },
    "IBCERC20": {
      "beacon": "<address>",
      "implementation": "<address>",
    },
  },
  "deployments": [
    {
      "contract": "<ICS26Router|ICS20Transfer|Escrow|IBCERC20>",
      "addresses": {
        "<proxy|beacon>": "<address>",
        "implementation": "<address>",
      },
      "init_info": {} // custom initialization information for contract
    }
  ]
}
```
