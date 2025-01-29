// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IIBCUpgradeableErrors } from "../errors/IIBCUpgradeableErrors.sol";
import { ContextUpgradeable } from "@openzeppelin-upgradeable/utils/ContextUpgradeable.sol";
import { UUPSUpgradeable } from "@openzeppelin-contracts/proxy/utils/UUPSUpgradeable.sol";

/// @title IBC Upgradeable contract
/// @notice This contract is an abstract contract for managing upgradeable IBC contracts
/// @dev This contract is meant to be used with OpenZeppelin's UUPS upgradeable proxy
/// @dev It manages two roles: the multisig admin, and the governance admin. The multisig admin represents a timelocked security council, and the governance admin represents an interchain account from the governance of a counterparty chain
/// @dev We highly recommend using `openzeppelin-contracts/contracts/governance/TimelockController.sol` for the multisig admin
/// @dev The multisig admin should be set during initialization, and the governance admin should be set later by the multisig admin
abstract contract IBCUpgradeable is IIBCUpgradeableErrors, UUPSUpgradeable, ContextUpgradeable {
    /// @notice Storage of the IBCUpgradeable contract
    /// @dev It's implemented on a custom ERC-7201 namespace to reduce the risk of storage collisions when using with upgradeable contracts.
    /// @param multisigAdmin The multisig admin address, assumed to be timelocked
    /// @param govAdmin The governance admin address
    struct IBCUpgradeableStorage {
        address multisigAdmin;
        address govAdmin;
    }

    /// @notice ERC-7201 slot for the IBCUpgradeable storage
    /// @dev keccak256(abi.encode(uint256(keccak256("ibc.storage.IBCUpgradeable")) - 1)) & ~bytes32(uint256(0xff))
    bytes32 private constant IBCUPGRADEABLE_STORAGE_SLOT =
        0xab89ef7f2323cbaa22f95c7c6ce118ac63015d8e94de63ba0e72d0e5d61d5800;

    /// @dev This contract is meant to be initialized with only the multisigAdmin, and the govAdmin should be set by the multisigAdmin later
    /// @dev It makes sense to have the multisigAdmin not be timelocked until the govAdmin is set
    function __IBCUpgradeable_init(address multisigAdmin) internal onlyInitializing {
        _getIBCUpgradeableStorage().multisigAdmin = multisigAdmin;
    }

    /// @notice Adds a governance admin address
    /// @dev The multisigAdmin should be timelocked after setting the govAdmin
    function addGovAdmin(address govAdmin) external onlyMultisigAdmin {
        IBCUpgradeableStorage storage $ = _getIBCUpgradeableStorage();
        require($.govAdmin == address(0), GovernanceAdminAlreadySet());
        $.govAdmin = govAdmin;
    }

    /// @notice Changes the multisig admin address
    /// @dev Governance admin can change the multisig admin address in case multisig admin is compromised
    /// @param newMultisigAdmin The new multisig admin address
    function changeMultisigAdmin(address newMultisigAdmin) external onlyAdmin {
        _getIBCUpgradeableStorage().multisigAdmin = newMultisigAdmin;
    }

    /// @inheritdoc UUPSUpgradeable
    function _authorizeUpgrade(address) internal virtual view override onlyAdmin {}

    /// @notice Returns the storage of the IBCUpgradeable contract
    function _getIBCUpgradeableStorage() internal pure returns (IBCUpgradeableStorage storage $) {
        // solhint-disable-next-line no-inline-assembly
        assembly {
            $.slot := IBCUPGRADEABLE_STORAGE_SLOT
        }
    }

    modifier onlyMultisigAdmin() {
        require(_msgSender() == _getIBCUpgradeableStorage().multisigAdmin, CallerNotMultisigAdmin());
        _;
    }

    modifier onlyGovAdmin() {
        require(_msgSender() == _getIBCUpgradeableStorage().govAdmin, CallerNotGovernanceAdmin());
        _;
    }

    modifier onlyAdmin() {
        IBCUpgradeableStorage storage $ = _getIBCUpgradeableStorage();
        require(_msgSender() == $.multisigAdmin || _msgSender() == $.govAdmin, Unauthorized());
        _;
    }
}
