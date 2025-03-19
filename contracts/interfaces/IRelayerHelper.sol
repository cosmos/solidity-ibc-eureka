// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IICS26RouterMsgs } from "../msgs/IICS26RouterMsgs.sol";

interface IRelayerHelper {
    /// @notice Calls the ICS26 router's recvPacket function with a gas limit
    /// @param msg_ The message to pass to the ICS26 router's recvPacket function
    /// @param gasLimit The gas limit
    function recvPacketWithGasLimit(IICS26RouterMsgs.MsgRecvPacket calldata msg_, uint256 gasLimit) external;

    /// @notice Calls the ICS26 router's ackPacket function with a gas limit
    /// @param msg_ The message to pass to the ICS26 router's ackPacket function
    /// @param gasLimit The gas limit
    function ackPacketWithGasLimit(IICS26RouterMsgs.MsgAckPacket calldata msg_, uint256 gasLimit) external;

    /// @notice Calls the ICS26 router's timeoutPacket function with a gas limit
    /// @param msg_ The message to pass to the ICS26 router's timeoutPacket function
    /// @param gasLimit The gas limit
    function timeoutPacketWithGasLimit(IICS26RouterMsgs.MsgTimeoutPacket calldata msg_, uint256 gasLimit) external;

    /// @notice Returns whether or not a packet was received
    /// @param packet The packet to check
    /// @return True if the packet was received, false otherwise
    function isPacketReceived(IICS26RouterMsgs.Packet calldata packet) external view returns (bool);

    /// @notice Returns whether or not a packet was received successfully
    /// @param packet The packet to check
    /// @return True if the packet was received and the application callback was successful, false otherwise
    function isPacketReceiveSuccessful(IICS26RouterMsgs.Packet calldata packet) external view returns (bool);

    /// @notice Returns the packet receipt for a given packet.
    /// @param clientId The packet destination client identifier.
    /// @param sequence The packet sequence number.
    /// @return The packet receipt for the given packet.
    function queryPacketReceipt(string calldata clientId, uint64 sequence) external view returns (bytes32);

    /// @notice Returns the packet commitment for a given packet.
    /// @param clientId The packet source client identifier.
    /// @param sequence The packet sequence number.
    /// @return The packet commitment for the given packet.
    function queryPacketCommitment(string calldata clientId, uint64 sequence) external view returns (bytes32);

    /// @notice Returns the packet acknowledgement commitment for a given packet.
    /// @param clientId The packet destination client identifier.
    /// @param sequence The packet sequence number.
    /// @return The packet acknowledgement commitment for the given packet.
    function queryAckCommitment(string calldata clientId, uint64 sequence) external view returns (bytes32);
}
