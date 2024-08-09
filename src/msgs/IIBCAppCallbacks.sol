// SPDX-License-Identifier: MIT
pragma solidity >=0.8.25;

import { IICS26RouterMsgs } from "./IICS26RouterMsgs.sol";

interface IIBCAppCallbacks {
    /// @notice Callback message for sending a packet.
    /// @dev The packet send is cancelled if the callback response is an error.
    /// @param packet The packet to be sent
    /// @param sender The sender of the packet
    struct OnSendPacketCallback {
        IICS26RouterMsgs.Packet packet;
        address sender;
    }

    /// @notice Callback message for receiving a packet.
    /// @param packet The packet to be received
    /// @param relayer The relayer of this message
    struct OnRecvPacketCallback {
        IICS26RouterMsgs.Packet packet;
        address relayer;
    }

    /// @notice Callback message for acknowledging a packet.
    /// @param packet The packet to be acknowledged
    /// @param acknowledgement The acknowledgement
    /// @param relayer The relayer of this message
    struct OnAcknowledgementPacketCallback {
        IICS26RouterMsgs.Packet packet;
        bytes acknowledgement;
        address relayer;
    }

    /// @notice Called when a packet is to be timed out by this IBC application.
    /// @param packet The packet to be timed out
    /// @param relayer The relayer of this message
    struct OnTimeoutPacketCallback {
        IICS26RouterMsgs.Packet packet;
        address relayer;
    }
}
