// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

/// @title IIBCPausable
/// @notice Interface for pausable IBC contracts for internal use.
interface IIBCPausable {
    /// @notice The role identifier for the pauser role
    /// @return The role identifier
    function PAUSER_ROLE() external view returns (bytes32);

    /// @notice The role identifier for the unpauser role
    /// @return The role identifier
    function UNPAUSER_ROLE() external view returns (bytes32);

    /// @notice Pauses the contract
    /// @dev The caller must have the pauser role
    function pause() external;

    /// @notice Unpauses the contract
    /// @dev The caller must have the unpauser role
    function unpause() external;

    /// @notice Grants the pauser role to an account
    /// @dev The caller must be authorized by the derived contract
    /// @param account The account to grant the role to
    function grantPauserRole(address account) external;

    /// @notice Revokes the pauser role from an account
    /// @dev The caller must be authorized by the derived contract
    /// @param account The account to revoke the role from
    function revokePauserRole(address account) external;

    /// @notice Grants the unpauser role to an account
    /// @dev The caller must be authorized by the derived contract
    /// @param account The account to grant the role to
    function grantUnpauserRole(address account) external;

    /// @notice Revokes the unpauser role from an account
    /// @dev The caller must be authorized by the derived contract
    /// @param account The account to revoke the role from
    function revokeUnpauserRole(address account) external;
}
