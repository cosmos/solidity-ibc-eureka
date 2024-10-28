// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

/// @title IBC Store Interface
interface IIBCStore {
    /// @notice Gets the commitment for a given path.
    /// @param hashedPath The hashed path to get the commitment for.
    /// @return The commitment for the given path.
    function getCommitment(bytes32 hashedPath) external view returns (bytes32);

    /// @notice Get the next sequence to send for a given port and channel pair.
    /// @param portId The port identifier.
    /// @param channelId The channel identifier.
    /// @return The next sequence to send.
    function getNextSequenceSend(string calldata portId, string calldata channelId) external view returns (uint32);
}
