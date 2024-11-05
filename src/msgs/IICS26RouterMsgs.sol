// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IICS02ClientMsgs } from "./IICS02ClientMsgs.sol";

interface IICS26RouterMsgs {
    /// @notice Packet struct
    /// @param sequence The sequence number of the packet
    /// @param timeoutTimestamp The timeout timestamp in the counterparty chain, in unix seconds
    /// @param sourcePort The source port identifier
    /// @param sourceChannel The source channel identifier (client id)
    /// @param destPort The destination port identifier
    /// @param destChannel The destination channel identifier
    /// @param version The version of the packet data
    /// @param data The packet data
    struct Packet {
        uint32 sequence;
        string sourceChannel;
        string destChannel;
        uint64 timeoutTimestamp;
        Payload[] payloads;
    }

    struct Payload {
       string sourcePort;
       string destPort;
       string version;
       string encoding;
       bytes value;
    }

    /// @notice Message for sending packets
    /// @dev Submitted by the user or the IBC application
    /// @param sourcePort The source port identifier
    /// @param sourceChannel The source channel identifier (client id)
    /// @param destPort The destination port identifier
    /// @param data The packet data
    /// @param timeoutTimestamp The timeout timestamp in unix seconds
    /// @param version The version of the packet data
    struct MsgSendPacket {
        string sourceChannel;
        uint64 timeoutTimestamp;
        Payload[] payloads;
    }

    /// @notice Message for receiving packets, submitted by relayer
    /// @param packet The packet to be received
    /// @param proofCommitment The proof of the packet commitment
    /// @param proofHeight The proof height
    struct MsgRecvPacket {
        Packet packet;
        bytes proofCommitment;
        IICS02ClientMsgs.Height proofHeight;
    }

    /// @notice Message for acknowledging packets, submitted by relayer
    /// @param packet The packet to be acknowledged
    /// @param acknowledgement The acknowledgement
    /// @param proofAcked The proof of the acknowledgement commitment
    /// @param proofHeight The proof height
    struct MsgAckPacket {
        Packet packet;
        bytes acknowledgement;
        bytes proofAcked;
        IICS02ClientMsgs.Height proofHeight;
    }

    /// @notice Message for timing out packets, submitted by relayer
    /// @param packet The packet to be timed out
    /// @param proofTimeout The proof of the packet commitment
    /// @param proofHeight The proof height
    struct MsgTimeoutPacket {
        Packet packet;
        bytes proofTimeout;
        IICS02ClientMsgs.Height proofHeight;
    }
}
