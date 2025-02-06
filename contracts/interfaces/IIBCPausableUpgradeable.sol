// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

interface IIBCPausableUpgradeable {
    /// @notice Returns the pauser address
    /// @return The pauser address
    function getPauser() external view returns (address);

    /// @notice Pauses the contract
    /// @dev The caller must be the pauser
    function pause() external;

    /// @notice Unpauses the contract
    /// @dev The caller must be the pauser
    function unpause() external;

    /// @notice Sets the pauser address
    /// @dev Must be authorized by this contract
    /// @dev This operation cannot be paused
    /// @param pauser The new pauser address
    function setPauser(address pauser) external;
}
