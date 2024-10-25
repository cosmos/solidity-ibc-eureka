// SPDX-License-Identifier: MIT
pragma solidity >=0.8.25;

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

    // --------------------- Events --------------------- //

    /// @notice Emitted when a packet is committed
    /// @param path The commitment path
    /// @param commitment The commitment data
    event PacketCommitted(bytes32 path, bytes32 commitment);

    /// @notice Emitted when an ack is commmitted 
    /// @param path The commitment path
    /// @param commitment The commitment data
    event AckCommitted(bytes32 path, bytes32 commitment);

}
