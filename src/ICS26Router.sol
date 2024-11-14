// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IIBCApp } from "./interfaces/IIBCApp.sol";
import { IICS26Router } from "./interfaces/IICS26Router.sol";
import { IICS02Client } from "./interfaces/IICS02Client.sol";
import { IICS04Channel } from "./interfaces/IICS04Channel.sol";
import { ICSCore } from "./ICSCore.sol";
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
import { IICS04ChannelMsgs } from "./msgs/IICS04ChannelMsgs.sol";
import { ReentrancyGuard } from "@openzeppelin/utils/ReentrancyGuard.sol";
import { ReentrancyGuardTransient } from "@openzeppelin/utils/ReentrancyGuardTransient.sol";
import { Multicall } from "@openzeppelin/utils/Multicall.sol";

/// @title IBC Eureka Router
/// @notice ICS26Router is the router for the IBC Eureka protocol
contract ICS26Router is IICS26Router, IICS26RouterErrors, Ownable, ReentrancyGuardTransient, Multicall {
    /// @dev portId => IBC Application contract
    mapping(string portId => IIBCApp app) private apps;

    /// @inheritdoc IICS26Router
    IICS02Client public immutable ICS02_CLIENT;
    /// @dev ICS04Channel contract
    IICS04Channel public immutable ICS04_CHANNEL;
    /// @inheritdoc IICS26Router
    IIBCStore public immutable IBC_STORE;

    constructor(address owner) Ownable(owner) {
        ICS02_CLIENT = new ICSCore(owner); // using the same owner
        ICS04_CHANNEL = IICS04Channel(address(ICS02_CLIENT)); // using the same contract
        IBC_STORE = new IBCStore(address(this)); // using this contract as the owner
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
        // TODO: Support multi-payload packets #93
        if (msg_.payloads.length != 1) {
            revert IBCMultiPayloadPacketNotSupported();
        }
        Payload calldata payload = msg_.payloads[0];

        string memory counterpartyId = ICS04_CHANNEL.getChannel(msg_.sourceChannel).counterpartyId;

        // TODO: validate all identifiers
        if (msg_.timeoutTimestamp <= block.timestamp) {
            revert IBCInvalidTimeoutTimestamp(msg_.timeoutTimestamp, block.timestamp);
        }

        uint32 sequence = IBC_STORE.nextSequenceSend(payload.sourcePort, msg_.sourceChannel);

        Packet memory packet = Packet({
            sequence: sequence,
            sourceChannel: msg_.sourceChannel,
            destChannel: counterpartyId,
            timeoutTimestamp: msg_.timeoutTimestamp,
            payloads: msg_.payloads
        });

        getIBCApp(payload.sourcePort).onSendPacket(
            IIBCAppCallbacks.OnSendPacketCallback({
                sourceChannel: msg_.sourceChannel,
                destinationChannel: counterpartyId,
                sequence: sequence,
                payload: payload,
                sender: _msgSender()
            })
        );

        IBC_STORE.commitPacket(packet);

        emit SendPacket(packet);
        return sequence;
    }

    /// @notice Receives a packet
    /// @param msg_ The message for receiving packets
    /// @inheritdoc IICS26Router
    function recvPacket(MsgRecvPacket calldata msg_) external nonReentrant {
       
        // TODO: Support multi-payload packets #93
        if (msg_.packet.payloads.length != 1) {
            revert IBCMultiPayloadPacketNotSupported();
        }
        Payload calldata payload = msg_.packet.payloads[0];

        IICS04ChannelMsgs.Channel memory channel = ICS04_CHANNEL.getChannel(msg_.packet.destChannel);
        if (keccak256(bytes(channel.counterpartyId)) != keccak256(bytes(msg_.packet.sourceChannel))) {
            revert IBCInvalidCounterparty(channel.counterpartyId, msg_.packet.sourceChannel);
        }

        if (msg_.packet.timeoutTimestamp <= block.timestamp) {
            revert IBCInvalidTimeoutTimestamp(msg_.packet.timeoutTimestamp, block.timestamp);
        }

        bytes memory commitmentPath =
            ICS24Host.packetCommitmentPathCalldata(msg_.packet.sourceChannel, msg_.packet.sequence);
        bytes32 commitmentBz = ICS24Host.packetCommitmentBytes32(msg_.packet);

        ILightClientMsgs.MsgMembership memory membershipMsg = ILightClientMsgs.MsgMembership({
            proof: msg_.proofCommitment,
            proofHeight: msg_.proofHeight,
            path: ICS24Host.prefixedPath(channel.merklePrefix, commitmentPath),
            value: abi.encodePacked(commitmentBz)
        });

        ICS02_CLIENT.getClient(msg_.packet.destChannel).membership(membershipMsg);

        bytes[] memory acks = new bytes[](1);
        acks[0] = getIBCApp(payload.destPort).onRecvPacket(
            IIBCAppCallbacks.OnRecvPacketCallback({
                sourceChannel: msg_.packet.sourceChannel,
                destinationChannel: msg_.packet.destChannel,
                sequence: msg_.packet.sequence,
                payload: payload,
                relayer: _msgSender()
            })
        );
        if (acks[0].length == 0) {
            revert IBCAsyncAcknowledgementNotSupported();
        }

        writeAcknowledgement(msg_.packet, acks);

        IBC_STORE.setPacketReceipt(msg_.packet);

        emit RecvPacket(msg_.packet);
    }

    /// @notice Acknowledges a packet
    /// @param msg_ The message for acknowledging packets
    /// @inheritdoc IICS26Router
    function ackPacket(MsgAckPacket calldata msg_) external nonReentrant {
        // TODO: Support multi-payload packets #93
        if (msg_.packet.payloads.length != 1) {
            revert IBCMultiPayloadPacketNotSupported();
        }
        Payload calldata payload = msg_.packet.payloads[0];

        IICS04ChannelMsgs.Channel memory channel = ICS04_CHANNEL.getChannel(msg_.packet.sourceChannel);
        if (keccak256(bytes(channel.counterpartyId)) != keccak256(bytes(msg_.packet.destChannel))) {
            revert IBCInvalidCounterparty(channel.counterpartyId, msg_.packet.destChannel);
        }

        // this will revert if the packet commitment does not exist
        bytes32 storedCommitment = IBC_STORE.deletePacketCommitment(msg_.packet);
        if (storedCommitment != ICS24Host.packetCommitmentBytes32(msg_.packet)) {
            revert IBCPacketCommitmentMismatch(storedCommitment, ICS24Host.packetCommitmentBytes32(msg_.packet));
        }

        bytes memory commitmentPath =
            ICS24Host.packetAcknowledgementCommitmentPathCalldata(msg_.packet.destChannel, msg_.packet.sequence);
        bytes[] memory acks = new bytes[](1);
        acks[0] = msg_.acknowledgement;
        bytes32 commitmentBz = ICS24Host.packetAcknowledgementCommitmentBytes32(acks);

        // verify the packet acknowledgement
        ILightClientMsgs.MsgMembership memory membershipMsg = ILightClientMsgs.MsgMembership({
            proof: msg_.proofAcked,
            proofHeight: msg_.proofHeight,
            path: ICS24Host.prefixedPath(channel.merklePrefix, commitmentPath),
            value: abi.encodePacked(commitmentBz)
        });

        ICS02_CLIENT.getClient(msg_.packet.sourceChannel).membership(membershipMsg);

        getIBCApp(payload.sourcePort).onAcknowledgementPacket(
            IIBCAppCallbacks.OnAcknowledgementPacketCallback({
                sourceChannel: msg_.packet.sourceChannel,
                destinationChannel: msg_.packet.destChannel,
                sequence: msg_.packet.sequence,
                payload: payload,
                acknowledgement: msg_.acknowledgement,
                relayer: _msgSender()
            })
        );

        emit AckPacket(msg_.packet, msg_.acknowledgement);
    }

    /// @notice Timeouts a packet
    /// @param msg_ The message for timing out packets
    /// @inheritdoc IICS26Router
    function timeoutPacket(MsgTimeoutPacket calldata msg_) external nonReentrant {
        
        // TODO: Support multi-payload packets #93
        if (msg_.packet.payloads.length != 1) {
            revert IBCMultiPayloadPacketNotSupported();
        }
        Payload calldata payload = msg_.packet.payloads[0];

        IICS04ChannelMsgs.Channel memory channel = ICS04_CHANNEL.getChannel(msg_.packet.sourceChannel);
        if (keccak256(bytes(channel.counterpartyId)) != keccak256(bytes(msg_.packet.destChannel))) {
            revert IBCInvalidCounterparty(channel.counterpartyId, msg_.packet.destChannel);
        }

        // this will revert if the packet commitment does not exist
        bytes32 storedCommitment = IBC_STORE.deletePacketCommitment(msg_.packet);
        if (storedCommitment != ICS24Host.packetCommitmentBytes32(msg_.packet)) {
            revert IBCPacketCommitmentMismatch(storedCommitment, ICS24Host.packetCommitmentBytes32(msg_.packet));
        }

        bytes memory receiptPath =
            ICS24Host.packetReceiptCommitmentPathCalldata(msg_.packet.destChannel, msg_.packet.sequence);
        ILightClientMsgs.MsgMembership memory nonMembershipMsg = ILightClientMsgs.MsgMembership({
            proof: msg_.proofTimeout,
            proofHeight: msg_.proofHeight,
            path: ICS24Host.prefixedPath(channel.merklePrefix, receiptPath),
            value: bytes("")
        });

        uint256 counterpartyTimestamp = ICS02_CLIENT.getClient(msg_.packet.sourceChannel).membership(nonMembershipMsg);
        if (counterpartyTimestamp < msg_.packet.timeoutTimestamp) {
            revert IBCInvalidTimeoutTimestamp(msg_.packet.timeoutTimestamp, counterpartyTimestamp);
        }

        getIBCApp(payload.sourcePort).onTimeoutPacket(
            IIBCAppCallbacks.OnTimeoutPacketCallback({
                sourceChannel: msg_.packet.sourceChannel,
                destinationChannel: msg_.packet.destChannel,
                sequence: msg_.packet.sequence,
                payload: payload,
                relayer: _msgSender()
            })
        );

        emit TimeoutPacket(msg_.packet);
    }

    /// @notice Writes a packet acknowledgement and emits an event
    /// @param packet The packet to acknowledge
    /// @param acks The acknowledgement
    function writeAcknowledgement(Packet calldata packet, bytes[] memory acks) private {
        IBC_STORE.commitPacketAcknowledgement(packet, acks);
        emit WriteAcknowledgement(packet, acks);
    }
}
