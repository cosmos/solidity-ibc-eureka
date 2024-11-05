// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { Strings } from "@openzeppelin/utils/Strings.sol";
import { IICS26RouterMsgs } from "../msgs/IICS26RouterMsgs.sol";
import { IICS24HostErrors } from "../errors/IICS24HostErrors.sol";
import { SafeCast } from "@openzeppelin/utils/math/SafeCast.sol";

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
    /// @param portId The port identifier
    /// @param channelId The channel identifier
    /// @param sequence The sequence number
    /// @return The full path of the packet commitment
    function packetCommitmentPathCalldata(
        string memory portId,
        string memory channelId,
        uint64 sequence
    )
        internal
        pure
        returns (bytes memory)
    {
        return abi.encodePacked(
            "commitments/ports/", portId, "/channels/", channelId, "/sequences/", Strings.toString(sequence)
        );
    }

    /// @notice Generator for the path of a packet acknowledgement commitment
    /// @param portId The port identifier
    /// @param channelId The channel identifier
    /// @param sequence The sequence number
    /// @return The full path of the packet acknowledgement commitment
    function packetAcknowledgementCommitmentPathCalldata(
        string memory portId,
        string memory channelId,
        uint64 sequence
    )
        internal
        pure
        returns (bytes memory)
    {
        return
            abi.encodePacked("acks/ports/", portId, "/channels/", channelId, "/sequences/", Strings.toString(sequence));
    }

    /// @notice Generator for the path of a packet receipt commitment
    /// @param portId The port identifier
    /// @param channelId The channel identifier
    /// @param sequence The sequence number
    /// @return The full path of the packet receipt commitment
    function packetReceiptCommitmentPathCalldata(
        string memory portId,
        string memory channelId,
        uint64 sequence
    )
        internal
        pure
        returns (bytes memory)
    {
        return abi.encodePacked(
            "receipts/ports/", portId, "/channels/", channelId, "/sequences/", Strings.toString(sequence)
        );
    }

    // Key generators for Commitment mapping

    /// @notice Generator for the key of a packet commitment
    /// @param portId The port identifier
    /// @param channelId The channel identifier
    /// @param sequence The sequence number
    /// @return The keccak256 hash of the packet commitment path
    function packetCommitmentKeyCalldata(
        string memory portId,
        string memory channelId,
        uint64 sequence
    )
        internal
        pure
        returns (bytes32)
    {
        return keccak256(packetCommitmentPathCalldata(portId, channelId, sequence));
    }

    /// @notice Generator for the key of a packet acknowledgement commitment
    /// @param portId The port identifier
    /// @param channelId The channel identifier
    /// @param sequence The sequence number
    /// @return The keccak256 hash of the packet acknowledgement commitment path
    function packetAcknowledgementCommitmentKeyCalldata(
        string memory portId,
        string memory channelId,
        uint64 sequence
    )
        internal
        pure
        returns (bytes32)
    {
        return keccak256(packetAcknowledgementCommitmentPathCalldata(portId, channelId, sequence));
    }

    /// @notice Generator for the key of a packet receipt commitment
    /// @param portId The port identifier
    /// @param channelId The channel identifier
    /// @param sequence The sequence number
    /// @return The keccak256 hash of the packet receipt commitment path
    function packetReceiptCommitmentKeyCalldata(
        string memory portId,
        string memory channelId,
        uint64 sequence
    )
        internal
        pure
        returns (bytes32)
    {
        return keccak256(packetReceiptCommitmentPathCalldata(portId, channelId, sequence));
    }

    /// @notice Get the packet commitment bytes.
    /// @param packet The packet to get the commitment for
    /// @return The commitment bytes
    function packetCommitmentBytes32(IICS26RouterMsgs.Packet memory packet) internal pure returns (bytes32) {
        return sha256(
            abi.encodePacked(
                SafeCast.toUint64(uint256(packet.timeoutTimestamp) * 1_000_000_000),
                uint64(0),
                uint64(0),
                sha256(packet.payloads[0].value),
                packet.payloads[0].destPort,
                packet.destChannel
            )
        );
    }

    /// @notice Get the packet acknowledgement commitment bytes.
    /// @param ack The acknowledgement to get the commitment for
    /// @return The commitment bytes
    function packetAcknowledgementCommitmentBytes32(bytes memory ack) internal pure returns (bytes32) {
        return sha256(ack);
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
