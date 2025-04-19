# Upgrading the Solidity Contracts

This repository houses the Solidity contracts for IBC v2, designed with upgradeability in mind. The core IBC contracts utilize the UUPS proxy pattern, ensuring controlled and efficient upgrades, while contracts deployed by the core IBC contracts follow the beacon proxy pattern for streamlined management and scalability.

| **Contract* | **Autority** | **Proxy Pattern** | **Upgrade Function** |
|:---:|:---:|:---:|:---:|
| `ICS26Router.sol` | [Admins](./README.md#security-assumptions) | [UUPSUpgradeable](https://docs.openzeppelin.com/contracts/5.x/api/proxy#UUPSUpgradeable) | `ICS26Router::upgradeToAndCall` |
| `ICS20Transfer.sol` | [Admins](./README.md#security-assumptions) | [UUPSUpgradeable](https://docs.openzeppelin.com/contracts/5.x/api/proxy#UUPSUpgradeable) | `ICS20Transfer::upgradeToAndCall` |
| `Escrow.sol` | `ICS20Transfer` | [Beacon](https://docs.openzeppelin.com/contracts/5.x/api/proxy#BeaconProxy) | `ICS20Transfer::upgradeEscrowTo` |
| `IBCERC20.sol` | `ICS20Transfer` | [Beacon](https://docs.openzeppelin.com/contracts/5.x/api/proxy#BeaconProxy) | `ICS20Transfer::upgradeIBCERC20To` |

![Light Mode Diagram](./docs/assets/upgradeability-light.svg#gh-light-mode-only)![Dark Mode Diagram](./docs/assets/upgradeability-dark.svg#gh-dark-mode-only)
