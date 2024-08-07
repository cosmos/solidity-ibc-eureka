// SPDX-License-Identifier: MIT
pragma solidity >=0.8.25;

import { Strings } from "@openzeppelin/contracts/utils/Strings.sol";
import { IICS26RouterMsgs } from "../msgs/IICS26RouterMsgs.sol";
import { SafeCast } from "@openzeppelin/contracts/utils/math/SafeCast.sol";

// @title ICS24 Host Path Generators
// @notice ICS24Host is a library that provides commitment path generators for ICS24 host requirements.
library ICS24Host {
    // Commitment generators that comply with
    // https://github.com/cosmos/ibc/tree/main/spec/core/ics-024-host-requirements#path-space

    // TODO: Figure out what a reasonable threshold is for the timestamp, how long into the future and still be safe
    uint256 public constant SECONDS_THRESHOLD = 7_952_338_800; // The year of our lord 2222

    enum PacketReceipt {
        NONE,
        SUCCESSFUL
    }

    bytes32 internal constant PACKET_RECEIPT_SUCCESSFUL_KECCAK256 =
        keccak256(abi.encodePacked(PacketReceipt.SUCCESSFUL));

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
    function packetCommitmentBytes32(IICS26RouterMsgs.Packet memory packet) internal pure returns (bytes32) {
        return sha256(
            abi.encodePacked(
                SafeCast.toUint64(uint256(packet.timeoutTimestamp) * 1_000_000_000),
                uint64(0),
                uint64(0),
                sha256(packet.data),
                packet.destPort,
                packet.destChannel
            )
        );
    }

    /// @notice Get the packet receipt commitment bytes.
    function packetAcknowledgementCommitmentBytes32(bytes memory ack) internal pure returns (bytes32) {
        return sha256(ack);
    }
}
