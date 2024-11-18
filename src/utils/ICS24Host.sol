// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IICS26RouterMsgs } from "../msgs/IICS26RouterMsgs.sol";
import { IICS24HostErrors } from "../errors/IICS24HostErrors.sol";

// @title ICS24 Host Path Generators
// @notice ICS24Host is a library that provides commitment path generators for ICS24 host requirements.
library ICS24Host {
    // Commitment generators that comply with
    // https://github.com/cosmos/ibc/tree/main/spec/core/ics-024-host-requirements#path-space

    /// @notice Packet receipt types
    enum PacketReceipt {
        NONE,
        SUCCESSFUL
    }

    /// @notice successful packet receipt
    bytes32 internal constant PACKET_RECEIPT_SUCCESSFUL_KECCAK256 =
        keccak256(abi.encodePacked(PacketReceipt.SUCCESSFUL));

    /// @notice Generator for the path of a packet commitment
    /// @param channelId The channel identifier
    /// @param sequence The sequence number
    /// @return The full path of the packet commitment
    function packetCommitmentPathCalldata(
        string memory channelId,
        uint64 sequence
    )
        internal
        pure
        returns (bytes memory)
    {
        return abi.encodePacked(channelId, uint8(1), uint64ToBigEndian(sequence));
    }

    /// @notice Generator for the path of a packet acknowledgement commitment
    /// @param channelId The channel identifier
    /// @param sequence The sequence number
    /// @return The full path of the packet acknowledgement commitment
    function packetAcknowledgementCommitmentPathCalldata(
        string memory channelId,
        uint64 sequence
    )
        internal
        pure
        returns (bytes memory)
    {
        return abi.encodePacked(channelId, uint8(3), uint64ToBigEndian(sequence));
    }

    /// @notice Generator for the path of a packet receipt commitment
    /// @param channelId The channel identifier
    /// @param sequence The sequence number
    /// @return The full path of the packet receipt commitment
    function packetReceiptCommitmentPathCalldata(
        string memory channelId,
        uint64 sequence
    )
        internal
        pure
        returns (bytes memory)
    {
        return abi.encodePacked(channelId, uint8(2), uint64ToBigEndian(sequence));
    }

    // Key generators for Commitment mapping

    /// @notice Generator for the key of a packet commitment
    /// @param channelId The channel identifier
    /// @param sequence The sequence number
    /// @return The keccak256 hash of the packet commitment path
    function packetCommitmentKeyCalldata(string memory channelId, uint64 sequence) internal pure returns (bytes32) {
        return keccak256(packetCommitmentPathCalldata(channelId, sequence));
    }

    /// @notice Generator for the key of a packet acknowledgement commitment
    /// @param channelId The channel identifier
    /// @param sequence The sequence number
    /// @return The keccak256 hash of the packet acknowledgement commitment path
    function packetAcknowledgementCommitmentKeyCalldata(
        string memory channelId,
        uint64 sequence
    )
        internal
        pure
        returns (bytes32)
    {
        return keccak256(packetAcknowledgementCommitmentPathCalldata(channelId, sequence));
    }

    /// @notice Generator for the key of a packet receipt commitment
    /// @param channelId The channel identifier
    /// @param sequence The sequence number
    /// @return The keccak256 hash of the packet receipt commitment path
    function packetReceiptCommitmentKeyCalldata(
        string memory channelId,
        uint64 sequence
    )
        internal
        pure
        returns (bytes32)
    {
        return keccak256(packetReceiptCommitmentPathCalldata(channelId, sequence));
    }

    /// @notice Get the packet commitment bytes.
    /// @dev CommitPacket returns the V2 packet commitment bytes. The commitment consists of:
    /// @dev sha256_hash(0x02 + sha256_hash(destinationChannel) + sha256_hash(timeout) + sha256_hash(payload)) for a
    /// @dev given packet.
    /// @dev This results in a fixed length preimage.
    /// @dev A fixed length preimage is ESSENTIAL to prevent relayers from being able
    /// @dev to malleate the packet fields and create a commitment hash that matches the original packet.
    /// @param packet The packet to get the commitment for
    /// @return The commitment bytes
    function packetCommitmentBytes32(IICS26RouterMsgs.Packet memory packet) internal pure returns (bytes32) {
        bytes memory appBytes = "";
        for (uint256 i = 0; i < packet.payloads.length; i++) {
            appBytes = abi.encodePacked(appBytes, hashPayload(packet.payloads[i]));
        }

        return sha256(
            abi.encodePacked(
                uint8(2),
                sha256(bytes(packet.destChannel)),
                sha256(abi.encodePacked(packet.timeoutTimestamp)),
                sha256(appBytes)
            )
        );
    }

    /// @notice Get the commitment hash of a payload
    /// @param data The payload to get the commitment hash for
    /// @return The commitment hash
    function hashPayload(IICS26RouterMsgs.Payload memory data) private pure returns (bytes32) {
        bytes memory buf = abi.encodePacked(
            sha256(bytes(data.sourcePort)),
            sha256(bytes(data.destPort)),
            sha256(bytes(data.version)),
            sha256(bytes(data.encoding)),
            sha256(data.value)
        );

        return sha256(buf);
    }

    /// @notice Get the packet acknowledgement commitment bytes.
    /// @dev PacketAcknowledgementCommitment returns the V2 packet acknowledgement commitment bytes.
    /// @dev The commitment consists of:
    /// @dev sha256_hash(0x02 + sha256_hash(ack1) + sha256_hash(ack2), ...) for a given set of acks.
    /// @dev each payload get one ack each from their application, so this function accepts a list of acks
    /// @param acks The list of acknowledgements to get the commitment for
    /// @return The commitment bytes
    function packetAcknowledgementCommitmentBytes32(bytes[] memory acks) internal pure returns (bytes32) {
        bytes memory ackBytes = "";
        for (uint256 i = 0; i < acks.length; i++) {
            ackBytes = abi.encodePacked(ackBytes, sha256(acks[i]));
        }

        return sha256(abi.encodePacked(uint8(2), ackBytes));
    }

    /// @notice Create a prefixed path
    /// @dev The path is appended to the last element of the prefix
    /// @param merklePrefix The prefix
    /// @param path The path to append
    /// @return The prefixed path
    function prefixedPath(bytes[] memory merklePrefix, bytes memory path) internal pure returns (bytes[] memory) {
        if (merklePrefix.length == 0) {
            revert IICS24HostErrors.InvalidMerklePrefix(merklePrefix);
        }

        merklePrefix[merklePrefix.length - 1] = abi.encodePacked(merklePrefix[merklePrefix.length - 1], path);
        return merklePrefix;
    }

    /// @notice Convert a uint64 to big endian bytes representation
    /// @param value The uint64 value
    /// @return The big endian bytes representation
    function uint64ToBigEndian(uint64 value) private pure returns (bytes8) {
        bytes8 result;
        // solhint-disable no-inline-assembly
        assembly {
            // Shift the uint64 value left by 192 bits to align with a bytes8's starting position
            result := shl(192, value)
        }
        return result;
    }
}
