// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IIBCPausableUpgradeableErrors } from "../errors/IIBCPausableUpgradeableErrors.sol";
import { IIBCPausableUpgradeable } from "../interfaces/IIBCPausableUpgradeable.sol";
import { ContextUpgradeable } from "@openzeppelin-upgradeable/utils/ContextUpgradeable.sol";
import { PausableUpgradeable } from "@openzeppelin-upgradeable/utils/PausableUpgradeable.sol";
import { AccessControlUpgradeable } from "@openzeppelin-upgradeable/access/AccessControlUpgradeable.sol";

/// @title IBC Pausable Upgradeable contract
/// @notice This contract is an abstract contract for adding pausability to IBC contracts.
abstract contract IBCPausableUpgradeable is
    IIBCPausableUpgradeableErrors,
    IIBCPausableUpgradeable,
    ContextUpgradeable,
    PausableUpgradeable,
    AccessControlUpgradeable
{
    /// @notice The role identifier for the pauser role
    bytes32 private constant PAUSER_ROLE = keccak256("PAUSER_ROLE");

    /// @dev Initializes the contract in unpaused state.
    /// @param pauser The address that can pause and unpause the contract
    function __IBCPausable_init(address pauser) internal onlyInitializing {
        __Pausable_init();

        _grantRole(PAUSER_ROLE, pauser);
    }

    /// @inheritdoc IIBCPausableUpgradeable
    function pause() external onlyRole(PAUSER_ROLE) {
        _pause();
    }

    /// @inheritdoc IIBCPausableUpgradeable
    function unpause() external onlyRole(PAUSER_ROLE) {
        _unpause();
    }

    /// @inheritdoc IIBCPausableUpgradeable
    function grantPauserRole(address account) external {
        _authorizeSetPauser(account);
        _grantRole(PAUSER_ROLE, account);
    }

    /// @inheritdoc IIBCPausableUpgradeable
    function revokePauserRole(address account) external {
        _authorizeSetPauser(account);
        _revokeRole(PAUSER_ROLE, account);
    }

    /// @notice Authorizes the setting of a new pauser
    /// @param pauser The new address that can pause and unpause the contract
    /// @dev This function must be overridden to add authorization logic
    function _authorizeSetPauser(address pauser) internal virtual;
}
