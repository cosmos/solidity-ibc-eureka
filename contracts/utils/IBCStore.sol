// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IIBCStore } from "../interfaces/IIBCStore.sol";
import { IICS26RouterMsgs } from "../msgs/IICS26RouterMsgs.sol";
import { ICS24Host } from "./ICS24Host.sol";
import { IICS24HostErrors } from "../errors/IICS24HostErrors.sol";
import { Ownable } from "@openzeppelin/access/Ownable.sol";

contract IBCStore is IIBCStore, IICS24HostErrors, Ownable {
    /// @notice all IBC commitments
    /// @dev keccak256(IBC-compatible-store-path) => sha256(IBC-compatible-commitment)
    mapping(bytes32 hashedPath => bytes32 commitment) internal commitments;

    /// @notice Previous sequence send for a given port and client pair
    /// @dev (portId, clientId) => prevSeqSend
    mapping(string clientId => uint32 prevSeqSend) private prevSequenceSends;

    /// @param owner_ The owner of the contract
    /// @dev Owner is to be the ICS26Router contract
    constructor(address owner_) Ownable(owner_) { }

    /// @inheritdoc IIBCStore
    function getCommitment(bytes32 hashedPath) public view returns (bytes32) {
        return commitments[hashedPath];
    }

    /// @inheritdoc IIBCStore
    function nextSequenceSend(string calldata clientId) public onlyOwner returns (uint32) {
        uint32 seq = prevSequenceSends[clientId] + 1;
        prevSequenceSends[clientId] = seq;
        return seq;
    }

    /// @inheritdoc IIBCStore
    function commitPacket(IICS26RouterMsgs.Packet memory packet) public onlyOwner {
        bytes32 path = ICS24Host.packetCommitmentKeyCalldata(packet.sourceClient, packet.sequence);
        require(
            commitments[path] == 0,
            IBCPacketCommitmentAlreadyExists(
                ICS24Host.packetCommitmentPathCalldata(packet.sourceClient, packet.sequence)
            )
        );

        bytes32 commitment = ICS24Host.packetCommitmentBytes32(packet);
        commitments[path] = commitment;
    }

    /// @inheritdoc IIBCStore
    function deletePacketCommitment(IICS26RouterMsgs.Packet memory packet) public onlyOwner returns (bytes32) {
        bytes32 path = ICS24Host.packetCommitmentKeyCalldata(packet.sourceClient, packet.sequence);
        bytes32 commitment = commitments[path];
        require(
            commitment != 0,
            IBCPacketCommitmentNotFound(ICS24Host.packetCommitmentPathCalldata(packet.sourceClient, packet.sequence))
        );

        delete commitments[path];
        return commitment;
    }

    /// @inheritdoc IIBCStore
    function setPacketReceipt(IICS26RouterMsgs.Packet memory packet) public onlyOwner {
        bytes32 path = ICS24Host.packetReceiptCommitmentKeyCalldata(packet.destClient, packet.sequence);
        require(
            commitments[path] == 0,
            IBCPacketReceiptAlreadyExists(
                ICS24Host.packetReceiptCommitmentPathCalldata(packet.destClient, packet.sequence)
            )
        );

        commitments[path] = ICS24Host.PACKET_RECEIPT_SUCCESSFUL_KECCAK256;
    }

    /// @inheritdoc IIBCStore
    function commitPacketAcknowledgement(IICS26RouterMsgs.Packet memory packet, bytes[] memory acks) public onlyOwner {
        bytes32 path = ICS24Host.packetAcknowledgementCommitmentKeyCalldata(packet.destClient, packet.sequence);
        require(
            commitments[path] == 0,
            IBCPacketAcknowledgementAlreadyExists(
                ICS24Host.packetAcknowledgementCommitmentPathCalldata(packet.destClient, packet.sequence)
            )
        );

        bytes32 commitment = ICS24Host.packetAcknowledgementCommitmentBytes32(acks);
        commitments[path] = commitment;
    }
}
