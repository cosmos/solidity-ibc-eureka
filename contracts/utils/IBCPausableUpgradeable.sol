// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IIBCPausable } from "../interfaces/IIBCPausable.sol";
import { ContextUpgradeable } from "@openzeppelin-upgradeable/utils/ContextUpgradeable.sol";
import { PausableUpgradeable } from "@openzeppelin-upgradeable/utils/PausableUpgradeable.sol";
import { AccessControlUpgradeable } from "@openzeppelin-upgradeable/access/AccessControlUpgradeable.sol";

/// @title IBC Pausable Upgradeable contract
/// @notice This contract is an abstract contract for adding pausability to IBC contracts.
abstract contract IBCPausableUpgradeable is
    IIBCPausable,
    ContextUpgradeable,
    PausableUpgradeable,
    AccessControlUpgradeable
{
    /// @inheritdoc IIBCPausable
    bytes32 public constant PAUSER_ROLE = keccak256("PAUSER_ROLE");
    /// @inheritdoc IIBCPausable
    bytes32 public constant UNPAUSER_ROLE = keccak256("UNPAUSER_ROLE");

    /// @dev Initializes the contract in unpaused state.
    /// @param pausers The addresses that are granted the `PAUSER_ROLE`
    /// @param unpausers The addresses that are granted the `UNPAUSER_ROLE`
    function __IBCPausable_init(address[] memory pausers, address[] memory unpausers) internal onlyInitializing {
        __Pausable_init();
        __AccessControl_init();

        for (uint256 i = 0; i < pausers.length; i++) {
            _grantRole(PAUSER_ROLE, pausers[i]);
        }

        for (uint256 i = 0; i < unpausers.length; i++) {
            _grantRole(UNPAUSER_ROLE, unpausers[i]);
        }
    }

    /// @inheritdoc IIBCPausable
    function pause() external onlyRole(PAUSER_ROLE) {
        _pause();
    }

    /// @inheritdoc IIBCPausable
    function unpause() external onlyRole(UNPAUSER_ROLE) {
        _unpause();
    }

    /// @inheritdoc IIBCPausable
    function grantPauserRole(address account) external {
        _authorizeSetPauser(account);
        _grantRole(PAUSER_ROLE, account);
    }

    /// @inheritdoc IIBCPausable
    function revokePauserRole(address account) external {
        _authorizeSetPauser(account);
        _revokeRole(PAUSER_ROLE, account);
    }

    /// @inheritdoc IIBCPausable
    function grantUnpauserRole(address account) external {
        _authorizeSetUnpauser(account);
        _grantRole(UNPAUSER_ROLE, account);
    }

    /// @inheritdoc IIBCPausable
    function revokeUnpauserRole(address account) external {
        _authorizeSetUnpauser(account);
        _revokeRole(UNPAUSER_ROLE, account);
    }

    /// @notice Authorizes the setting of a new pauser
    /// @param pauser The new address that can pause the contract
    /// @dev This function must be overridden to add authorization logic
    function _authorizeSetPauser(address pauser) internal virtual;

    /// @notice Authorizes the setting of a new unpauser
    /// @param unpauser The new address that can unpause the contract
    /// @dev This function must be overridden to add authorization logic
    function _authorizeSetUnpauser(address unpauser) internal virtual;
}
