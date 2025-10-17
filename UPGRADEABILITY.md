# Upgradeability

This repository houses both Solidity contracts and Solana programs for IBC v2, designed with upgradeability in mind.

## Solidity Contracts

The core IBC contracts utilize the UUPS proxy pattern, ensuring controlled and efficient upgrades, while contracts deployed by the core IBC contracts follow the beacon proxy pattern for streamlined management and scalability.

| **Contract** | **Authority** | **Proxy Pattern** | **Upgrade Function** |
|:---:|:---:|:---:|:---:|
| `ICS26Router.sol` | [Admins](./README.md#security-assumptions) | [UUPSUpgradeable](https://docs.openzeppelin.com/contracts/5.x/api/proxy#UUPSUpgradeable) | `ICS26Router::upgradeToAndCall` |
| `ICS20Transfer.sol` | [Admins](./README.md#security-assumptions) | [UUPSUpgradeable](https://docs.openzeppelin.com/contracts/5.x/api/proxy#UUPSUpgradeable) | `ICS20Transfer::upgradeToAndCall` |
| `Escrow.sol` | `ICS20Transfer` | [Beacon](https://docs.openzeppelin.com/contracts/5.x/api/proxy#BeaconProxy) | `ICS20Transfer::upgradeEscrowTo` |
| `IBCERC20.sol` | `ICS20Transfer` | [Beacon](https://docs.openzeppelin.com/contracts/5.x/api/proxy#BeaconProxy) | `ICS20Transfer::upgradeIBCERC20To` |

![Light Mode Diagram](./docs/assets/upgradeability-light.svg#gh-light-mode-only)![Dark Mode Diagram](./docs/assets/upgradeability-dark.svg#gh-dark-mode-only)

## Solana Programs

Solana programs are deployed using the BPF Loader Upgradeable, which allows the program's executable code to be upgraded while preserving existing program-derived addresses (PDAs) and account data.


### Account State Versioning

All persistent account structures include version fields (using type-safe enums) and reserved space to support future upgrades:

**ics26-router:**
- `RouterState`: `AccountVersion` enum (V1) + 256 bytes reserved space
- `Client`: `AccountVersion` enum (V1) + 256 bytes reserved space
- `IBCApp`: `AccountVersion` enum (V1) + 256 bytes reserved space

This versioning strategy allows:
- **Backward compatibility**: Old accounts can be identified and migrated
- **Future expansion**: New fields can be added using reserved space
- **Safe upgrades**: Program code can check version and handle different account layouts
