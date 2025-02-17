# Upgrading the Solidity Contracts

This repository contains the Solidity contracts for IBC Eureka. These contracts are upgradeable through proxies. The UUPS proxy pattern is used for the core IBC contracts, while the beacon proxy pattern is used for contracts that are deployed by the core IBC contracts.

| **Contract* | **Autority** | **Proxy Pattern** | **Upgrade Function** |
|:---:|:---:|:---:|:---:|
| `ICS26Router.sol` | [Admins](./README.md#security-assumptions) | [UUPSUpgradeable](https://docs.openzeppelin.com/contracts/5.x/api/proxy#UUPSUpgradeable) | `ICS26Router::upgradeToAndCall` |
| `ICS20Transfer.sol` | [Admins](./README.md#security-assumptions) | [UUPSUpgradeable](https://docs.openzeppelin.com/contracts/5.x/api/proxy#UUPSUpgradeable) | `ICS20Transfer::upgradeToAndCall` |
| `Escrow.sol` | `ICS20Transfer` | [Beacon](https://docs.openzeppelin.com/contracts/5.x/api/proxy#BeaconProxy) | `ICS20Transfer::upgradeEscrowTo` |
| `IBCERC20.sol` | `ICS20Transfer` | [Beacon](https://docs.openzeppelin.com/contracts/5.x/api/proxy#BeaconProxy) | `ICS20Transfer::upgradeIBCERC20To` |
