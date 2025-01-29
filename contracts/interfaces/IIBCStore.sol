// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

/// @title IBC Store Interface
/// @dev Non-view functions can only be called by owner.
interface IIBCStore {
    /// @notice Gets the commitment for a given path.
    /// @param hashedPath The hashed path to get the commitment for.
    /// @return The commitment for the given path.
    function getCommitment(bytes32 hashedPath) external view returns (bytes32);
}
