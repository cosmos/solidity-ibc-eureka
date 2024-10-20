// SPDX-License-Identifier: MIT
pragma solidity >=0.8.25;

import { IIBCApp } from "./interfaces/IIBCApp.sol";
import { IICS26Router } from "./interfaces/IICS26Router.sol";
import { IICS02Client } from "./interfaces/IICS02Client.sol";
import { IIBCStore } from "./interfaces/IIBCStore.sol";
import { IBCStore } from "./utils/IBCStore.sol";
import { IICS26RouterErrors } from "./errors/IICS26RouterErrors.sol";
import { Ownable } from "@openzeppelin/access/Ownable.sol";
import { Strings } from "@openzeppelin/utils/Strings.sol";
import { IBCIdentifiers } from "./utils/IBCIdentifiers.sol";
import { IIBCAppCallbacks } from "./msgs/IIBCAppCallbacks.sol";
import { ICS24Host } from "./utils/ICS24Host.sol";
import { ILightClientMsgs } from "./msgs/ILightClientMsgs.sol";
import { IICS02ClientMsgs } from "./msgs/IICS02ClientMsgs.sol";
import { ReentrancyGuard } from "@openzeppelin/utils/ReentrancyGuard.sol";

/// @title IBC Eureka Router
/// @notice ICS26Router is the router for the IBC Eureka protocol
contract ICS26Router is IICS26Router, Ownable, IICS26RouterErrors, ReentrancyGuard {
    /// @dev portId => IBC Application contract
    mapping(string portId => IIBCApp app) private apps;
    /// @dev ICS02Client contract
    IICS02Client private ics02Client;
    /// @dev IBC Storage Contract
    IIBCStore public immutable IBC_STORE;

    constructor(address ics02Client_, address owner) Ownable(owner) {
        ics02Client = IICS02Client(ics02Client_);
        IBC_STORE = new IBCStore(address(this));
    }

    /// @notice Returns the address of the IBC application given the port identifier
    /// @param portId The port identifier
    /// @return The address of the IBC application contract
    /// @inheritdoc IICS26Router
    function getIBCApp(string calldata portId) public view returns (IIBCApp) {
        IIBCApp app = apps[portId];
        if (app == IIBCApp(address(0))) {
            revert IBCAppNotFound(portId);
        }
        return app;
    }

    /// @notice Adds an IBC application to the router
    /// @dev Only the admin can submit non-empty port identifiers
    /// @param portId The port identifier
    /// @param app The address of the IBC application contract
    /// @inheritdoc IICS26Router
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

        emit IBCAppAdded(newPortId, app);
    }

    /// @notice Sends a packet
    /// @param msg_ The message for sending packets
    /// @return The sequence number of the packet
    /// @inheritdoc IICS26Router
    function sendPacket(MsgSendPacket calldata msg_) external nonReentrant returns (uint32) {
        string memory counterpartyId = ics02Client.getCounterparty(msg_.sourceChannel).clientId;

        // TODO: validate all identifiers
        if (msg_.timeoutTimestamp <= block.timestamp) {
            revert IBCInvalidTimeoutTimestamp(msg_.timeoutTimestamp, block.timestamp);
        }

        uint32 sequence = IBC_STORE.nextSequenceSend(msg_.sourcePort, msg_.sourceChannel);

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

        getIBCApp(msg_.sourcePort).onSendPacket(
            IIBCAppCallbacks.OnSendPacketCallback({ packet: packet, sender: msg.sender })
        );

        IBC_STORE.commitPacket(packet);

        emit SendPacket(packet);
        return sequence;
    }

    /// @notice Receives a packet
    /// @param msg_ The message for receiving packets
    /// @inheritdoc IICS26Router
    function recvPacket(MsgRecvPacket calldata msg_) external nonReentrant {
        IICS02ClientMsgs.CounterpartyInfo memory cInfo = ics02Client.getCounterparty(msg_.packet.destChannel);
        if (keccak256(bytes(cInfo.clientId)) != keccak256(bytes(msg_.packet.sourceChannel))) {
            revert IBCInvalidCounterparty(cInfo.clientId, msg_.packet.sourceChannel);
        }

        if (msg_.packet.timeoutTimestamp <= block.timestamp) {
            revert IBCInvalidTimeoutTimestamp(msg_.packet.timeoutTimestamp, block.timestamp);
        }

        bytes memory commitmentPath = ICS24Host.packetCommitmentPathCalldata(
            msg_.packet.sourcePort, msg_.packet.sourceChannel, msg_.packet.sequence
        );
        bytes32 commitmentBz = ICS24Host.packetCommitmentBytes32(msg_.packet);

        ILightClientMsgs.MsgMembership memory membershipMsg = ILightClientMsgs.MsgMembership({
            proof: msg_.proofCommitment,
            proofHeight: msg_.proofHeight,
            path: ICS24Host.prefixedPath(cInfo.merklePrefix, commitmentPath),
            value: abi.encodePacked(commitmentBz)
        });

        ics02Client.getClient(msg_.packet.destChannel).membership(membershipMsg);

        bytes memory ack = getIBCApp(msg_.packet.destPort).onRecvPacket(
            IIBCAppCallbacks.OnRecvPacketCallback({ packet: msg_.packet, relayer: msg.sender })
        );
        if (ack.length == 0) {
            revert IBCAsyncAcknowledgementNotSupported();
        }

        writeAcknowledgement(msg_.packet, ack);

        IBC_STORE.setPacketReceipt(msg_.packet);

        emit RecvPacket(msg_.packet);
    }

    /// @notice Acknowledges a packet
    /// @param msg_ The message for acknowledging packets
    /// @inheritdoc IICS26Router
    function ackPacket(MsgAckPacket calldata msg_) external nonReentrant {
        IICS02ClientMsgs.CounterpartyInfo memory cInfo = ics02Client.getCounterparty(msg_.packet.sourceChannel);
        if (keccak256(bytes(cInfo.clientId)) != keccak256(bytes(msg_.packet.destChannel))) {
            revert IBCInvalidCounterparty(cInfo.clientId, msg_.packet.destChannel);
        }

        // this will revert if the packet commitment does not exist
        bytes32 storedCommitment = IBC_STORE.deletePacketCommitment(msg_.packet);
        if (storedCommitment != ICS24Host.packetCommitmentBytes32(msg_.packet)) {
            revert IBCPacketCommitmentMismatch(storedCommitment, ICS24Host.packetCommitmentBytes32(msg_.packet));
        }

        bytes memory commitmentPath = ICS24Host.packetAcknowledgementCommitmentPathCalldata(
            msg_.packet.destPort, msg_.packet.destChannel, msg_.packet.sequence
        );
        bytes32 commitmentBz = ICS24Host.packetAcknowledgementCommitmentBytes32(msg_.acknowledgement);

        // verify the packet acknowledgement
        ILightClientMsgs.MsgMembership memory membershipMsg = ILightClientMsgs.MsgMembership({
            proof: msg_.proofAcked,
            proofHeight: msg_.proofHeight,
            path: ICS24Host.prefixedPath(cInfo.merklePrefix, commitmentPath),
            value: abi.encodePacked(commitmentBz)
        });

        ics02Client.getClient(msg_.packet.sourceChannel).membership(membershipMsg);

        getIBCApp(msg_.packet.sourcePort).onAcknowledgementPacket(
            IIBCAppCallbacks.OnAcknowledgementPacketCallback({
                packet: msg_.packet,
                acknowledgement: msg_.acknowledgement,
                relayer: msg.sender
            })
        );

        emit AckPacket(msg_.packet, msg_.acknowledgement);
    }

    /// @notice Timeouts a packet
    /// @param msg_ The message for timing out packets
    /// @inheritdoc IICS26Router
    function timeoutPacket(MsgTimeoutPacket calldata msg_) external nonReentrant {
        IICS02ClientMsgs.CounterpartyInfo memory cInfo = ics02Client.getCounterparty(msg_.packet.sourceChannel);
        if (keccak256(bytes(cInfo.clientId)) != keccak256(bytes(msg_.packet.destChannel))) {
            revert IBCInvalidCounterparty(cInfo.clientId, msg_.packet.destChannel);
        }

        // this will revert if the packet commitment does not exist
        bytes32 storedCommitment = IBC_STORE.deletePacketCommitment(msg_.packet);
        if (storedCommitment != ICS24Host.packetCommitmentBytes32(msg_.packet)) {
            revert IBCPacketCommitmentMismatch(storedCommitment, ICS24Host.packetCommitmentBytes32(msg_.packet));
        }

        bytes memory receiptPath = ICS24Host.packetReceiptCommitmentPathCalldata(
            msg_.packet.destPort, msg_.packet.destChannel, msg_.packet.sequence
        );
        ILightClientMsgs.MsgMembership memory nonMembershipMsg = ILightClientMsgs.MsgMembership({
            proof: msg_.proofTimeout,
            proofHeight: msg_.proofHeight,
            path: ICS24Host.prefixedPath(cInfo.merklePrefix, receiptPath),
            value: bytes("")
        });

        uint256 counterpartyTimestamp = ics02Client.getClient(msg_.packet.sourceChannel).membership(nonMembershipMsg);
        if (counterpartyTimestamp < msg_.packet.timeoutTimestamp) {
            revert IBCInvalidTimeoutTimestamp(msg_.packet.timeoutTimestamp, counterpartyTimestamp);
        }

        getIBCApp(msg_.packet.sourcePort).onTimeoutPacket(
            IIBCAppCallbacks.OnTimeoutPacketCallback({ packet: msg_.packet, relayer: msg.sender })
        );

        emit TimeoutPacket(msg_.packet);
    }

    /// @notice Writes a packet acknowledgement and emits an event
    /// @param packet The packet to acknowledge
    /// @param ack The acknowledgement
    function writeAcknowledgement(Packet calldata packet, bytes memory ack) private {
        IBC_STORE.commitPacketAcknowledgement(packet, ack);
        emit WriteAcknowledgement(packet, ack);
    }
}
