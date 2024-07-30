// SPDX-License-Identifier: MIT
pragma solidity >=0.8.25;

import { IICS26RouterMsgs } from "./IICS26RouterMsgs.sol";

interface IIBCAppCallbacks {
    /// OnSendPacket is called when a packet send request is received by the router.
    /// The packet send is cancelled if the callback response is an error.
    struct OnSendPacketCallback {
        /// The packet to be sent
        IICS26RouterMsgs.Packet packet;
        /// The sender of the packet
        address sender;
    }

    /// Called when a packet is sent to this IBC application.
    struct OnRecvPacketCallback {
        /// The packet to be received
        IICS26RouterMsgs.Packet packet;
        /// The relayer of this message
        address relayer;
    }

    /// Called when a packet is to be acknowledged by this IBC application.
    /// This callback need not be responded with data.
    struct OnAcknowledgementPacketCallback {
        /// The packet to be acknowledged
        IICS26RouterMsgs.Packet packet;
        /// The acknowledgement
        bytes acknowledgement;
        /// The relayer of this message
        address relayer;
    }

    /// Called when a packet is to be timed out by this IBC application.
    /// This callback need not be responded with data.
    struct OnTimeoutPacketCallback {
        /// The packet to be timed out
        IICS26RouterMsgs.Packet packet;
        /// The relayer of this message
        address relayer;
    }
}
