// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IIBCStore } from "../interfaces/IIBCStore.sol";
import { IICS26RouterMsgs } from "../msgs/IICS26RouterMsgs.sol";
import { ICS24Host } from "./ICS24Host.sol";
import { IICS24HostErrors } from "../errors/IICS24HostErrors.sol";
import { Ownable } from "@openzeppelin/access/Ownable.sol";

contract IBCStore is IIBCStore, IICS24HostErrors, Ownable {
    /// @notice all IBC commitments
    /// @dev keccak256(IBC-compatible-store-path) => keccak256(IBC-compatible-commitment)
    mapping(bytes32 hashedPath => bytes32 commitment) internal commitments;

    /// @notice Previous sequence send for a given port and channel pair
    /// @dev (portId, channelId) => prevSeqSend
    mapping(string portId => mapping(string channelId => uint32 prevSeqSend)) private prevSequenceSends;

    /// @param owner_ The owner of the contract
    /// @dev Owner is to be the ICS26Router contract
    constructor(address owner_) Ownable(owner_) { }

    /// @inheritdoc IIBCStore
    function getCommitment(bytes32 hashedPath) public view returns (bytes32) {
        return commitments[hashedPath];
    }

    /// @notice Gets and increments the next sequence to send for a given port and channel pair.
    /// @param portId The port identifier
    /// @param channelId The channel identifier
    /// @return The next sequence to send
    /// @inheritdoc IIBCStore
    function nextSequenceSend(string calldata portId, string calldata channelId) public onlyOwner returns (uint32) {
        uint32 seq = prevSequenceSends[portId][channelId] + 1;
        prevSequenceSends[portId][channelId] = seq;
        return seq;
    }

    /// @notice Commits a packet
    /// @param packet The packet to commit
    /// @custom:spec
    /// https://github.com/cosmos/ibc-go/blob/2b40562bcd59ce820ddd7d6732940728487cf94e/
    /// modules/core/04-channel/types/packet.go#L38
    /// @inheritdoc IIBCStore
    function commitPacket(IICS26RouterMsgs.Packet memory packet) public onlyOwner {
        bytes32 path = ICS24Host.packetCommitmentKeyCalldata(packet.sourcePort, packet.sourceChannel, packet.sequence);
        if (commitments[path] != 0) {
            revert IBCPacketCommitmentAlreadyExists(
                ICS24Host.packetCommitmentPathCalldata(packet.sourcePort, packet.sourceChannel, packet.sequence)
            );
        }

        bytes32 commitment = ICS24Host.packetCommitmentBytes32(packet);
        commitments[path] = commitment;
    }

    /// @inheritdoc IIBCStore
    function deletePacketCommitment(IICS26RouterMsgs.Packet memory packet) public onlyOwner returns (bytes32) {
        bytes32 path = ICS24Host.packetCommitmentKeyCalldata(packet.sourcePort, packet.sourceChannel, packet.sequence);
        bytes32 commitment = commitments[path];
        if (commitment == 0) {
            revert IBCPacketCommitmentNotFound(
                ICS24Host.packetCommitmentPathCalldata(packet.sourcePort, packet.sourceChannel, packet.sequence)
            );
        }

        delete commitments[path];
        return commitment;
    }

    /// @inheritdoc IIBCStore
    function setPacketReceipt(IICS26RouterMsgs.Packet memory packet) public onlyOwner {
        bytes32 path =
            ICS24Host.packetReceiptCommitmentKeyCalldata(packet.destPort, packet.destChannel, packet.sequence);
        if (commitments[path] != 0) {
            revert IBCPacketReceiptAlreadyExists(
                ICS24Host.packetReceiptCommitmentPathCalldata(packet.destPort, packet.destChannel, packet.sequence)
            );
        }

        commitments[path] = ICS24Host.PACKET_RECEIPT_SUCCESSFUL_KECCAK256;
    }

    /// @inheritdoc IIBCStore
    function commitPacketAcknowledgement(IICS26RouterMsgs.Packet memory packet, bytes memory ack) public onlyOwner {
        bytes32 path =
            ICS24Host.packetAcknowledgementCommitmentKeyCalldata(packet.destPort, packet.destChannel, packet.sequence);
        if (commitments[path] != 0) {
            revert IBCPacketAcknowledgementAlreadyExists(
                ICS24Host.packetAcknowledgementCommitmentPathCalldata(
                    packet.destPort, packet.destChannel, packet.sequence
                )
            );
        }

        bytes32 commitment = ICS24Host.packetAcknowledgementCommitmentBytes32(ack);
        commitments[path] = commitment;
    }
}
