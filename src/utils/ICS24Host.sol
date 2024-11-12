// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { Strings } from "@openzeppelin/utils/Strings.sol";
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
        return abi.encodePacked("commitments/channels/", channelId, "/sequences/", Strings.toString(sequence));
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
        return abi.encodePacked("acks/channels/", channelId, "/sequences/", Strings.toString(sequence));
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
        return abi.encodePacked("receipts/channels/", channelId, "/sequences/", Strings.toString(sequence));
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
    /// @param packet The packet to get the commitment for
    /// @return The commitment bytes
    function packetCommitmentBytes32(IICS26RouterMsgs.Packet memory packet) internal pure returns (bytes32) {
        // TODO: Support multi-payload packets #93
        if (packet.payloads.length != 1) {
            revert IICS24HostErrors.IBCMultiPayloadPacketNotSupported();
        }

        return sha256(
            abi.encodePacked(
                packet.timeoutTimestamp, sha256(bytes(packet.destChannel)), hashPayload(packet.payloads[0])
            )
        );
    }

    function hashPayload(IICS26RouterMsgs.Payload memory data) internal pure returns (bytes32) {
        bytes memory buf = abi.encodePacked(
            sha256(bytes(data.sourcePort)),
            sha256(bytes(data.destPort)),
            sha256(data.value),
            sha256(bytes(data.encoding)),
            sha256(bytes(data.version))
        );

        return sha256(buf);
    }

    /// @notice Get the packet acknowledgement commitment bytes.
    /// @dev each payload get one ack each from their application, so this function accepts a list of acks
    /// @param acks The list of acknowledgements to get the commitment for
    /// @return The commitment bytes
    function packetAcknowledgementCommitmentBytes32(bytes[] memory acks) internal pure returns (bytes32) {
        // TODO: Support multi-payload packets #93
        if (acks.length != 1) {
            revert IICS24HostErrors.IBCMultiPayloadPacketNotSupported();
        }

        return sha256(abi.encodePacked(sha256(acks[0])));
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
}
