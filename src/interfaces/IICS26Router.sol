// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.25;

import { IICS26RouterMsgs } from "../msgs/IICS26RouterMsgs.sol";
import { IIBCApp } from "./IIBCApp.sol";

/// @title ICS26 Router Interface
/// @notice IICS26Router is an interface for the IBC Eureka router
interface IICS26Router is IICS26RouterMsgs {
    /// @notice Returns the address of the IBC application given the port identifier
    /// @param portId The port identifier
    /// @return The address of the IBC application contract
    function getIBCApp(string calldata portId) external view returns (IIBCApp);

    /// @notice Adds an IBC application to the router
    /// @dev Only the admin can submit non-empty port identifiers. The default port identifier
    // is the address of the IBC application contract.
    /// @param portId The port identifier, only admin can submit non-empty port identifiers.
    /// @param app The address of the IBC application contract
    function addIBCApp(string calldata portId, address app) external;

    /// @notice Sends a packet
    /// @param msg The message for sending packets
    /// @return The sequence number of the packet
    function sendPacket(MsgSendPacket calldata msg) external returns (uint32);

    /// @notice Receives a packet
    /// @param msg The message for receiving packets
    function recvPacket(MsgRecvPacket calldata msg) external;

    /// @notice Acknowledges a packet
    /// @param msg The message for acknowledging packets
    function ackPacket(MsgAckPacket calldata msg) external;

    /// @notice Timeouts a packet
    /// @param msg The message for timing out packets
    function timeoutPacket(MsgTimeoutPacket calldata msg) external;

    // --------------------- Events --------------------- //

    /// @notice Emitted when a packet is sent
    event SendPacket(Packet msg);
    /// @notice Emitted when a packet is received
    event RecvPacket(Packet msg);
    /// @notice Emitted when a packet is acknowledged
    event WriteAcknowledgement(Packet msg, bytes acknowledgement);
    /// @notice Emitted when a packet is timed out
    event TimeoutPacket(Packet msg);
}
