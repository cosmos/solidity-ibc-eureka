// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.25;

import {IICS02ClientMsgs} from "./IICS02ClientMsgs.sol";

interface IICS26RouterMsgs {
    // @notice Packet struct
    struct Packet {
        /// The sequence number of the packet. Each packet transmitted in a channel
        uint32 sequence;
        /// Timeout timestamp in the counterparty chain, e.g., unix timestamp in seconds for tendermint.
        uint32 timeoutTimestamp;
        /// The source port identifier
        string sourcePort;
        /// The source channel identifier, this is a client id in IBC Eureka
        string sourceChannel;
        /// The destination port identifier
        string destPort;
        /// The destination channel identifier, this may be a client id in IBC Eureka
        string destChannel;
        /// The version of the packet data
        string version;
        /// The packet data
        bytes data;
    }

    // @notice Message for sending packets
    struct MsgSendPacket {
        /// The source port identifier
        string sourcePort;
        /// The source channel identifier, this is a client id in IBC Eureka
        string sourceChannel;
        /// The destination port identifier
        string destPort;
        /// The packet data
        bytes data;
        /// The timeout timestamp in the counterparty chain, e.g., unix timestamp in seconds for tendermint.
        uint32 timeoutTimestamp;
        /// The version of the packet data
        string version;
    }

    // @notice Message for receiving packets, submitted by relayer
    struct MsgRecvPacket {
        /// The packet to be received
        Packet packet;
        /// The proof of the packet commitment
        bytes proofCommitment;
        /// The proof height
        IICS02ClientMsgs.Height proofHeight;
    }

    // @notice Message for acknowledging packets, submitted by relayer
    struct MsgAcknowledgement {
        /// The packet to be acknowledged
        Packet packet;
        /// The acknowledgement
        bytes acknowledgement;
        /// The proof of the acknowledgement commitment
        bytes proofAcked;
        /// The proof height
        IICS02ClientMsgs.Height proofHeight;
    }

    // @notice Message for timing out packets, submitted by relayer
    struct MsgTimeout {
        /// The packet to be timed out
        Packet packet;
        /// The proof of the packet commitment
        bytes proofTimeout;
        /// The proof height
        IICS02ClientMsgs.Height proofHeight;
    }

}
