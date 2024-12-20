// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IIBCApp } from "./interfaces/IIBCApp.sol";
import { IICS26Router } from "./interfaces/IICS26Router.sol";
import { IICS02Client } from "./interfaces/IICS02Client.sol";
import { IICS04Channel } from "./interfaces/IICS04Channel.sol";
import { ICSCore } from "./ICSCore.sol";
import { IIBCStore } from "./interfaces/IIBCStore.sol";
import { IICS24HostErrors } from "./errors/IICS24HostErrors.sol";
import { IBCStore } from "./utils/IBCStore.sol";
import { IICS26RouterErrors } from "./errors/IICS26RouterErrors.sol";
import { Ownable } from "@openzeppelin/access/Ownable.sol";
import { Strings } from "@openzeppelin/utils/Strings.sol";
import { IBCIdentifiers } from "./utils/IBCIdentifiers.sol";
import { IIBCAppCallbacks } from "./msgs/IIBCAppCallbacks.sol";
import { ICS24Host } from "./utils/ICS24Host.sol";
import { ILightClientMsgs } from "./msgs/ILightClientMsgs.sol";
import { IICS04ChannelMsgs } from "./msgs/IICS04ChannelMsgs.sol";
import { ReentrancyGuardTransient } from "@openzeppelin/utils/ReentrancyGuardTransient.sol";
import { Multicall } from "@openzeppelin/utils/Multicall.sol";
import { Initializable } from "@openzeppelin/proxy/utils/Initializable.sol";

/// @title IBC Eureka Router
/// @notice ICS26Router is the router for the IBC Eureka protocol
contract ICS26Router is
    Initializable,
    IICS26Router,
    IICS26RouterErrors,
    Ownable,
    ReentrancyGuardTransient,
    Multicall
{
    /// @dev portId => IBC Application contract
    mapping(string portId => IIBCApp app) private apps;

    /// @inheritdoc IICS26Router
    /// @dev Supposed to be immutable, but we need to set it in the initializer
    IIBCStore public IBC_STORE;
    /// @notice ICSCore implements IICS02Client and IICS04Channel
    /// @dev Supposed to be immutable, but we need to set it in the initializer
    address private ICS_CORE;

    /// @dev This contract is meant to be deployed by a proxy, so the constructor is not used
    constructor() Ownable(address(0xdead)) {
        _disableInitializers();
    }

    /// @notice Initializes the contract instead of a constructor
    /// @dev Meant to be called only once from the proxy
    /// @param owner_ The owner of the contract
    /// @param icsCore The address of the ICSCore contract
    function initialize(address owner_, address icsCore) public initializer {
        _transferOwnership(owner_);
        ICS_CORE = icsCore; // using the same owner
        IBC_STORE = new IBCStore(address(this)); // using this contract as the owner
    }

    /// @inheritdoc IICS26Router
    function ICS02_CLIENT() public view returns (IICS02Client) {
        return IICS02Client(ICS_CORE);
    }

    /// @inheritdoc IICS26Router
    function ICS04_CHANNEL() public view returns (IICS04Channel) {
        return IICS04Channel(ICS_CORE);
    }

    /// @notice Returns the address of the IBC application given the port identifier
    /// @param portId The port identifier
    /// @return The address of the IBC application contract
    /// @inheritdoc IICS26Router
    function getIBCApp(string calldata portId) public view returns (IIBCApp) {
        IIBCApp app = apps[portId];
        require(address(app) != address(0), IBCAppNotFound(portId));
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

        require(address(apps[newPortId]) == address(0), IBCPortAlreadyExists(newPortId));
        require(IBCIdentifiers.validatePortIdentifier(bytes(newPortId)), IBCInvalidPortIdentifier(newPortId));

        apps[newPortId] = IIBCApp(app);

        emit IBCAppAdded(newPortId, app);
    }

    /// @notice Sends a packet
    /// @param msg_ The message for sending packets
    /// @return The sequence number of the packet
    /// @inheritdoc IICS26Router
    function sendPacket(MsgSendPacket calldata msg_) external nonReentrant returns (uint32) {
        // TODO: Support multi-payload packets #93
        require(msg_.payloads.length == 1, IBCMultiPayloadPacketNotSupported());
        Payload calldata payload = msg_.payloads[0];

        string memory counterpartyId = ICS04_CHANNEL().getChannel(msg_.sourceChannel).counterpartyId;

        // TODO: validate all identifiers
        require(
            msg_.timeoutTimestamp > block.timestamp, IBCInvalidTimeoutTimestamp(msg_.timeoutTimestamp, block.timestamp)
        );

        uint32 sequence = IBC_STORE.nextSequenceSend(msg_.sourceChannel);

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
        require(msg_.packet.payloads.length == 1, IBCMultiPayloadPacketNotSupported());
        Payload calldata payload = msg_.packet.payloads[0];

        IICS04ChannelMsgs.Channel memory channel = ICS04_CHANNEL().getChannel(msg_.packet.destChannel);
        require(
            keccak256(bytes(channel.counterpartyId)) == keccak256(bytes(msg_.packet.sourceChannel)),
            IBCInvalidCounterparty(channel.counterpartyId, msg_.packet.sourceChannel)
        );

        require(
            msg_.packet.timeoutTimestamp > block.timestamp,
            IBCInvalidTimeoutTimestamp(msg_.packet.timeoutTimestamp, block.timestamp)
        );

        bytes memory commitmentPath =
            ICS24Host.packetCommitmentPathCalldata(msg_.packet.sourceChannel, msg_.packet.sequence);
        bytes32 commitmentBz = ICS24Host.packetCommitmentBytes32(msg_.packet);

        ILightClientMsgs.MsgMembership memory membershipMsg = ILightClientMsgs.MsgMembership({
            proof: msg_.proofCommitment,
            proofHeight: msg_.proofHeight,
            path: ICS24Host.prefixedPath(channel.merklePrefix, commitmentPath),
            value: abi.encodePacked(commitmentBz)
        });

        ICS02_CLIENT().getClient(msg_.packet.destChannel).membership(membershipMsg);

        // recvPacket will no-op if the packet receipt already exists
        // solhint-disable-next-line no-empty-blocks
        try IBC_STORE.setPacketReceipt(msg_.packet) { }
        catch (bytes memory reason) {
            return noopOnCorrectReason(reason, IICS24HostErrors.IBCPacketReceiptAlreadyExists.selector);
        }

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
        require(acks[0].length != 0, IBCAsyncAcknowledgementNotSupported());

        writeAcknowledgement(msg_.packet, acks);

        emit RecvPacket(msg_.packet);
    }

    /// @notice Acknowledges a packet
    /// @param msg_ The message for acknowledging packets
    /// @inheritdoc IICS26Router
    function ackPacket(MsgAckPacket calldata msg_) external nonReentrant {
        // TODO: Support multi-payload packets #93
        require(msg_.packet.payloads.length == 1, IBCMultiPayloadPacketNotSupported());
        Payload calldata payload = msg_.packet.payloads[0];

        IICS04ChannelMsgs.Channel memory channel = ICS04_CHANNEL().getChannel(msg_.packet.sourceChannel);
        require(
            keccak256(bytes(channel.counterpartyId)) == keccak256(bytes(msg_.packet.destChannel)),
            IBCInvalidCounterparty(channel.counterpartyId, msg_.packet.destChannel)
        );

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

        ICS02_CLIENT().getClient(msg_.packet.sourceChannel).membership(membershipMsg);

        // ackPacket will no-op if the packet commitment does not exist
        try IBC_STORE.deletePacketCommitment(msg_.packet) returns (bytes32 storedCommitment) {
            require(
                storedCommitment == ICS24Host.packetCommitmentBytes32(msg_.packet),
                IBCPacketCommitmentMismatch(storedCommitment, ICS24Host.packetCommitmentBytes32(msg_.packet))
            );
        } catch (bytes memory reason) {
            return noopOnCorrectReason(reason, IICS24HostErrors.IBCPacketCommitmentNotFound.selector);
        }

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
        require(msg_.packet.payloads.length == 1, IBCMultiPayloadPacketNotSupported());
        Payload calldata payload = msg_.packet.payloads[0];

        IICS04ChannelMsgs.Channel memory channel = ICS04_CHANNEL().getChannel(msg_.packet.sourceChannel);
        require(
            keccak256(bytes(channel.counterpartyId)) == keccak256(bytes(msg_.packet.destChannel)),
            IBCInvalidCounterparty(channel.counterpartyId, msg_.packet.destChannel)
        );

        bytes memory receiptPath =
            ICS24Host.packetReceiptCommitmentPathCalldata(msg_.packet.destChannel, msg_.packet.sequence);
        ILightClientMsgs.MsgMembership memory nonMembershipMsg = ILightClientMsgs.MsgMembership({
            proof: msg_.proofTimeout,
            proofHeight: msg_.proofHeight,
            path: ICS24Host.prefixedPath(channel.merklePrefix, receiptPath),
            value: bytes("")
        });

        uint256 counterpartyTimestamp = ICS02_CLIENT().getClient(msg_.packet.sourceChannel).membership(nonMembershipMsg);
        require(
            counterpartyTimestamp >= msg_.packet.timeoutTimestamp,
            IBCInvalidTimeoutTimestamp(msg_.packet.timeoutTimestamp, counterpartyTimestamp)
        );

        // timeoutPacket will no-op if the packet commitment does not exist
        try IBC_STORE.deletePacketCommitment(msg_.packet) returns (bytes32 storedCommitment) {
            require(
                storedCommitment == ICS24Host.packetCommitmentBytes32(msg_.packet),
                IBCPacketCommitmentMismatch(storedCommitment, ICS24Host.packetCommitmentBytes32(msg_.packet))
            );
        } catch (bytes memory reason) {
            return noopOnCorrectReason(reason, IICS24HostErrors.IBCPacketCommitmentNotFound.selector);
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

    /// @notice No-op if the reason is correct, otherwise reverts with the same reason
    /// @dev Only to be used in catch blocks
    /// @param reason The reason to check
    /// @param correctReason The correct reason
    function noopOnCorrectReason(bytes memory reason, bytes4 correctReason) private {
        if (bytes4(reason) == correctReason) {
            emit Noop();
        } else {
            // reverts with the same reason
            // solhint-disable-next-line no-inline-assembly
            assembly ("memory-safe") {
                revert(add(reason, 32), mload(reason))
            }
        }
    }
}
