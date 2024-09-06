// SPDX-License-Identifier: MIT
pragma solidity >=0.8.25;

import { IICS26RouterMsgs } from "../msgs/IICS26RouterMsgs.sol";

interface IICS26RouterEvents is IICS26RouterMsgs {
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
    /// @param acknowledgement The acknowledgement data
    event WriteAcknowledgement(Packet packet, bytes acknowledgement);

    /// @notice Emitted when a packet is timed out
    /// @param packet The packet that was timed out
    event TimeoutPacket(Packet packet);

    /// @notice Emitted when a packet is acknowledged
    /// @param packet The packet that was acknowledged
    /// @param acknowledgement The acknowledgement data
    event AckPacket(Packet packet, bytes acknowledgement);
}
