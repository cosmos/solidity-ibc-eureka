// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IICS26RouterMsgs } from "./IICS26RouterMsgs.sol";

interface IIBCAppCallbacks {
    /// @notice Callback message for sending a packet.
    /// @dev The packet send is cancelled if the callback response is an error.
    /// @param sourceChannel The source channel identifier
    /// @param destinationChannel The destination channel identifier
    /// @param sequence The sequence number of the packet
    /// @param payload The packet payload
    /// @param sender The sender of the packet
    struct OnSendPacketCallback {
        string sourceChannel;
        string destinationChannel;
        uint64 sequence;
        IICS26RouterMsgs.Payload payload;
        address sender;
    }

    /// @notice Callback message for receiving a packet.
    /// @param sourceChannel The source channel identifier
    /// @param destinationChannel The destination channel identifier
    /// @param sequence The sequence number of the packet
    /// @param payload The packet payload
    /// @param relayer The relayer of this message
    struct OnRecvPacketCallback {
        string sourceChannel;
        string destinationChannel;
        uint64 sequence;
        IICS26RouterMsgs.Payload payload;
        address relayer;
    }

    /// @notice Callback message for acknowledging a packet.
    /// @param sourceChannel The source channel identifier
    /// @param destinationChannel The destination channel identifier
    /// @param sequence The sequence number of the packet
    /// @param payload The packet payload
    /// @param recvSuccess the success boolean flag of the receive packet on counterparty
    /// @param acknowledgement The acknowledgement
    /// @param relayer The relayer of this message
    struct OnAcknowledgementPacketCallback {
        string sourceChannel;
        string destinationChannel;
        uint64 sequence;
        IICS26RouterMsgs.Payload payload;
        bool recvSuccess;
        bytes acknowledgement;
        address relayer;
    }

    /// @notice Called when a packet is to be timed out by this IBC application.
    /// @param sourceChannel The source channel identifier
    /// @param destinationChannel The destination channel identifier
    /// @param sequence The sequence number of the packet
    /// @param payload The packet payload
    /// @param relayer The relayer of this message
    struct OnTimeoutPacketCallback {
        string sourceChannel;
        string destinationChannel;
        uint64 sequence;
        IICS26RouterMsgs.Payload payload;
        address relayer;
    }
}
