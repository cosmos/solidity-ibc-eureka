// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IICS26RouterMsgs } from "../msgs/IICS26RouterMsgs.sol";

/// @title IBC Store Interface
/// @dev Non-view functions can only be called by owner.
interface IIBCStore {
    /// @notice Gets the commitment for a given path.
    /// @param hashedPath The hashed path to get the commitment for.
    /// @return The commitment for the given path.
    function getCommitment(bytes32 hashedPath) external view returns (bytes32);

    /// @notice Gets and increments the next sequence to send for a given port and channel pair.
    /// @param portId The port identifier.
    /// @param channelId The channel identifier.
    /// @return The next sequence to send.
    function nextSequenceSend(string calldata portId, string calldata channelId) external returns (uint32);

    /// @notice Commits a packet
    /// @param packet The packet to commit
    function commitPacket(IICS26RouterMsgs.Packet memory packet) external;

    /// @notice Deletes a packet commitment and reverts if it does not exist
    /// @param packet The packet whose commitment to delete
    /// @return The deleted packet commitment
    function deletePacketCommitment(IICS26RouterMsgs.Packet memory packet) external returns (bytes32);

    /// @notice Sets a packet receipt
    /// @param packet The packet to set the receipt for
    function setPacketReceipt(IICS26RouterMsgs.Packet memory packet) external;

    /// @notice Commits a packet acknowledgement
    /// @param packet The packet to commit the acknowledgement for
    /// @param ack The acknowledgement to commit
    function commitPacketAcknowledgement(IICS26RouterMsgs.Packet memory packet, bytes memory ack) external;
}
