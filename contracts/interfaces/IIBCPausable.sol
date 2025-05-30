// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

/// @title IIBCPausable
/// @notice Interface for pausable IBC contracts for internal use.
interface IIBCPausable {
    /// @notice Pauses the contract
    /// @dev This call is restricted
    function pause() external;

    /// @notice Unpauses the contract
    /// @dev This call is restricted
    function unpause() external;
}
