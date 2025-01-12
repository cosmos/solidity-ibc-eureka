// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IICS26RouterMsgs } from "../msgs/IICS26RouterMsgs.sol";
import { IIBCApp } from "./IIBCApp.sol";
import { IIBCStore } from "./IIBCStore.sol";
import { IICS02Client } from "./IICS02Client.sol";
import { IICS04Channel } from "./IICS04Channel.sol";

/// @title ICS26 Router Interface
/// @notice IICS26Router is an interface for the IBC Eureka router
interface IICS26Router is IICS26RouterMsgs {
    /// @notice Returns the IBC storage contract
    /// @return The address of the IBC stotage contract
    function IBC_STORE() external view returns (IIBCStore);

    /// @notice Returns the ICS02 client contract
    /// @return The address of the ICS02 client contract
    function ICS02_CLIENT() external view returns (IICS02Client);

    /// @notice Returns the ICS04 channel contract
    /// @return The address of the ICS04 channel contract
    function ICS04_CHANNEL() external view returns (IICS04Channel);

    /// @notice Returns the address of the IBC application given the port identifier
    /// @param portId The port identifier
    /// @return The address of the IBC application contract
    function getIBCApp(string calldata portId) external view returns (IIBCApp);

    /// @notice Adds an IBC application to the router
    /// @dev Only the admin can submit non-empty port identifiers.
    /// @dev The default port identifier is the address of the IBC application contract.
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

    /// @notice Emitted when an IBC application is added to the router
    /// @param portId The port identifier
    /// @param app The address of the IBC application contract
    event IBCAppAdded(string portId, address app);
    /// @notice Emitted when a packet is sent
    /// @param packet The sent packet
    event SendPacket(Packet packet);
    /// @notice Emitted when a packet is received
    /// @param packet The received packet
    event RecvPacket(Packet packet);
    /// @notice Emitted when a packet acknowledgement is written
    /// @param packet The packet that was acknowledged
    /// @param acknowledgements The list of acknowledgements data
    event WriteAcknowledgement(Packet packet, bytes[] acknowledgements);
    /// @notice Emitted when a packet is timed out
    /// @param packet The packet that was timed out
    event TimeoutPacket(Packet packet);
    /// @notice Emitted when a packet is acknowledged
    /// @param packet The packet that was acknowledged
    /// @param acknowledgement The acknowledgement data
    event AckPacket(Packet packet, bytes acknowledgement);
    /// @notice Emitted when a redundant relay occurs
    event Noop();
}
