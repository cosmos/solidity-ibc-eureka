// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.25;

import { IIBCApp } from "./interfaces/IIBCApp.sol";
import { IICS26Router } from "./interfaces/IICS26Router.sol";
import { IICS02Client } from "./interfaces/IICS02Client.sol";
import { IBCStore } from "./utils/IBCStore.sol";
import { IICS26RouterErrors } from "./errors/IICS26RouterErrors.sol";
import { Ownable } from "@openzeppelin/contracts/access/Ownable.sol";
import { Strings } from "@openzeppelin/contracts/utils/Strings.sol";
import { IBCIdentifiers } from "./utils/IBCIdentifiers.sol";
import { IIBCAppCallbacks } from "./msgs/IIBCAppCallbacks.sol";
import { ICS24Host } from "./utils/ICS24Host.sol";
import { ILightClientMsgs } from "./msgs/ILightClientMsgs.sol";
import "@openzeppelin/contracts/utils/ReentrancyGuard.sol";

/// @title IBC Eureka Router
/// @notice ICS26Router is the router for the IBC Eureka protocol
contract ICS26Router is IICS26Router, IBCStore, Ownable, IICS26RouterErrors, ReentrancyGuard {
    mapping(string => IIBCApp) private apps;
    IICS02Client private ics02Client;

    constructor(address ics02Client_, address owner) Ownable(owner) {
        ics02Client = IICS02Client(ics02Client_);
    }

    /// @notice Returns the address of the IBC application given the port identifier
    /// @param portId The port identifier
    /// @return The address of the IBC application contract
    function getIBCApp(string calldata portId) external view returns (IIBCApp) {
        return apps[portId];
    }

    /// @notice Adds an IBC application to the router
    /// @dev Only the admin can submit non-empty port identifiers
    /// @param portId The port identifier
    /// @param app The address of the IBC application contract
    function addIBCApp(string calldata portId, address app) external {
        string memory newPortId;
        if (bytes(portId).length != 0) {
            Ownable._checkOwner();
            newPortId = portId;
        } else {
            newPortId = Strings.toHexString(app);
        }

        if (apps[newPortId] != IIBCApp(address(0))) {
            revert IBCPortAlreadyExists(newPortId);
        }
        if (!IBCIdentifiers.validatePortIdentifier(bytes(newPortId))) {
            revert IBCInvalidPortIdentifier(newPortId);
        }

        apps[newPortId] = IIBCApp(app);
    }

    /// @notice Sends a packet
    /// @param msg_ The message for sending packets
    /// @return The sequence number of the packet
    function sendPacket(MsgSendPacket calldata msg_) external nonReentrant returns (uint32) {
        string memory counterpartyId = ics02Client.getCounterparty(msg_.sourcePort).clientId;

        // TODO: validate all identifiers
        if (msg_.timeoutTimestamp <= block.timestamp) {
            revert IBCInvalidTimeoutTimestamp(msg_.timeoutTimestamp, block.timestamp);
        }

        uint32 sequence = IBCStore.nextSequenceSend(msg_.sourcePort, msg_.sourceChannel);

        Packet memory packet = Packet({
            sequence: sequence,
            timeoutTimestamp: msg_.timeoutTimestamp,
            sourcePort: msg_.sourcePort,
            sourceChannel: msg_.sourceChannel,
            destPort: msg_.destPort,
            destChannel: counterpartyId,
            version: msg_.version,
            data: msg_.data
        });

        IIBCAppCallbacks.OnSendPacketCallback memory sendPacketCallback =
            IIBCAppCallbacks.OnSendPacketCallback({ packet: packet, sender: msg.sender });

        apps[msg_.sourcePort].onSendPacket(sendPacketCallback);

        IBCStore.commitPacket(packet);

        emit SendPacket(packet);
        return sequence;
    }

    /// @notice Receives a packet
    /// @param msg_ The message for receiving packets
    function recvPacket(MsgRecvPacket calldata msg_) external nonReentrant {
        IIBCApp app = apps[msg_.packet.destPort];

        string memory counterpartyId = ics02Client.getCounterparty(msg_.packet.destChannel).clientId;
        if (keccak256(bytes(counterpartyId)) != keccak256(bytes(msg_.packet.sourceChannel))) {
            revert IBCInvalidCounterparty(counterpartyId, msg_.packet.sourceChannel);
        }

        bytes memory commitmentPath = ICS24Host.packetCommitmentPathCalldata(
            msg_.packet.sourcePort, msg_.packet.sourceChannel, msg_.packet.sequence
        );
        bytes32 commitmentBz = ICS24Host.packetCommitmentBytes32(msg_.packet);

        ILightClientMsgs.MsgMembership memory membershipMsg = ILightClientMsgs.MsgMembership({
            proof: msg_.proofCommitment,
            proofHeight: msg_.proofHeight,
            kvPair: ILightClientMsgs.KVPair({ path: commitmentPath, value: abi.encodePacked(commitmentBz) })
        });

        ics02Client.getClient(msg_.packet.destChannel).verifyMembership(membershipMsg);
        if (msg_.packet.timeoutTimestamp <= block.timestamp) {
            revert IBCInvalidTimeoutTimestamp(msg_.packet.timeoutTimestamp, block.timestamp);
        }

        bytes memory ack =
            app.onRecvPacket(IIBCAppCallbacks.OnRecvPacketCallback({ packet: msg_.packet, relayer: msg.sender }));
        if (ack.length == 0) {
            revert IBCAsyncAcknowledgementNotSupported();
        }

        writeAcknowledgement(msg_.packet, ack);

        emit RecvPacket(msg_.packet);
    }

    /// @notice Acknowledges a packet
    /// @param msg_ The message for acknowledging packets
    function ackPacket(MsgAckPacket calldata msg_) external nonReentrant {
        // TODO: implement
        // IIBCApp app = IIBCApp(apps[msg.packet.sourcePort]);
    }

    /// @notice Timeouts a packet
    /// @param msg_ The message for timing out packets
    function timeoutPacket(MsgTimeoutPacket calldata msg_) external nonReentrant {
        // TODO: implement
        // IIBCApp app = IIBCApp(apps[msg.packet.sourcePort]);
    }

    /// @notice Writes a packet acknowledgement and emits an event
    function writeAcknowledgement(Packet calldata packet, bytes memory ack) private {
        IBCStore.commitPacketAcknowledgement(packet, ack);
        emit WriteAcknowledgement(packet, ack);
    }
}
