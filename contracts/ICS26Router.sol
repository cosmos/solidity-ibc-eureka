// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { ILightClientMsgs } from "./msgs/ILightClientMsgs.sol";
import { IICS26RouterMsgs } from "./msgs/IICS26RouterMsgs.sol";
import { IICS02ClientMsgs } from "./msgs/IICS02ClientMsgs.sol";
import { IIBCAppCallbacks } from "./msgs/IIBCAppCallbacks.sol";

import { IICS26RouterErrors } from "./errors/IICS26RouterErrors.sol";
import { IIBCApp } from "./interfaces/IIBCApp.sol";
import { IICS26Router } from "./interfaces/IICS26Router.sol";

import { ReentrancyGuardTransientUpgradeable } from
    "@openzeppelin-upgradeable/utils/ReentrancyGuardTransientUpgradeable.sol";
import { IBCStoreUpgradeable } from "./utils/IBCStoreUpgradeable.sol";
import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";
import { IBCIdentifiers } from "./utils/IBCIdentifiers.sol";
import { ICS24Host } from "./utils/ICS24Host.sol";
import { ICS02ClientUpgradeable } from "./utils/ICS02ClientUpgradeable.sol";
import { MulticallUpgradeable } from "@openzeppelin-upgradeable/utils/MulticallUpgradeable.sol";
import { IBCUUPSUpgradeable } from "./utils/IBCUUPSUpgradeable.sol";

/// @title IBC Eureka Router
/// @notice ICS26Router is the router for the IBC Eureka protocol
contract ICS26Router is
    IICS26RouterErrors,
    IICS26Router,
    ICS02ClientUpgradeable,
    IBCStoreUpgradeable,
    ReentrancyGuardTransientUpgradeable,
    MulticallUpgradeable,
    IBCUUPSUpgradeable
{
    /// @notice Storage of the ICS26Router contract
    /// @dev It's implemented on a custom ERC-7201 namespace to reduce the risk of storage collisions when using with
    /// upgradeable contracts.
    /// @param apps The mapping of port identifiers to IBC application contracts
    /// @param ics02Client The ICS02Client contract
    /// @custom:storage-location erc7201:ibc.storage.ICS26Router
    struct ICS26RouterStorage {
        mapping(string => IIBCApp) apps;
    }

    /// @notice ERC-7201 slot for the ICS26Router storage
    /// @dev keccak256(abi.encode(uint256(keccak256("ibc.storage.ICS26Router")) - 1)) & ~bytes32(uint256(0xff))
    bytes32 private constant ICS26ROUTER_STORAGE_SLOT =
        0xc5779f3c2c21083eefa6d04f6a698bc0d8c10db124ad5e0df6ef394b6d7bf600;

    /// @dev The maximum timeout duration for a packet
    uint256 private constant MAX_TIMEOUT_DURATION = 1 days;

    /// @dev The role identifier for the port customizer role
    /// @dev The port identifier role is used to add IBC applications with custom port identifiers
    bytes32 private constant PORT_CUSTOMIZER_ROLE = keccak256("PORT_CUSTOMIZER_ROLE");

    /// @dev This contract is meant to be deployed by a proxy, so the constructor is not used
    constructor() {
        _disableInitializers();
    }

    /// @notice Initializes the contract instead of a constructor
    /// @dev Meant to be called only once from the proxy
    /// @param timelockedAdmin The address of the timelocked admin for IBCUUPSUpgradeable
    /// @param portCustomizer The address of the port customizer
    function initialize(address timelockedAdmin, address portCustomizer) public initializer {
        __AccessControl_init();
        __ReentrancyGuardTransient_init();
        __Multicall_init();
        __ICS02Client_init();
        __IBCStoreUpgradeable_init();
        __IBCUUPSUpgradeable_init(timelockedAdmin);

        _grantRole(PORT_CUSTOMIZER_ROLE, portCustomizer);
    }

    /// @notice Returns the address of the IBC application given the port identifier
    /// @param portId The port identifier
    /// @return The address of the IBC application contract
    /// @inheritdoc IICS26Router
    function getIBCApp(string calldata portId) public view returns (IIBCApp) {
        IIBCApp app = _getICS26RouterStorage().apps[portId];
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
            _checkRole(PORT_CUSTOMIZER_ROLE);
            newPortId = portId;
        } else {
            newPortId = Strings.toHexString(app);
        }

        ICS26RouterStorage storage $ = _getICS26RouterStorage();

        require(address($.apps[newPortId]) == address(0), IBCPortAlreadyExists(newPortId));
        require(IBCIdentifiers.validatePortIdentifier(bytes(newPortId)), IBCInvalidPortIdentifier(newPortId));

        $.apps[newPortId] = IIBCApp(app);

        emit IBCAppAdded(newPortId, app);
    }

    /// @notice Sends a packet
    /// @param msg_ The message for sending packets
    /// @return The sequence number of the packet
    /// @inheritdoc IICS26Router
    function sendPacket(IICS26RouterMsgs.MsgSendPacket calldata msg_) external nonReentrant returns (uint32) {
        address ibcApp = address(getIBCApp(msg_.payload.sourcePort));
        require(ibcApp == _msgSender(), IBCUnauthorizedSender(_msgSender()));

        string memory counterpartyId = getCounterparty(msg_.sourceClient).clientId;

        // TODO: validate all identifiers
        require(
            msg_.timeoutTimestamp > block.timestamp, IBCInvalidTimeoutTimestamp(msg_.timeoutTimestamp, block.timestamp)
        );
        require(
            msg_.timeoutTimestamp - block.timestamp <= MAX_TIMEOUT_DURATION,
            IBCInvalidTimeoutDuration(MAX_TIMEOUT_DURATION, msg_.timeoutTimestamp - block.timestamp)
        );

        uint32 sequence = nextSequenceSend(msg_.sourceClient);

        // TODO: Support multi-payload packets #93
        IICS26RouterMsgs.Packet memory packet = IICS26RouterMsgs.Packet({
            sequence: sequence,
            sourceClient: msg_.sourceClient,
            destClient: counterpartyId,
            timeoutTimestamp: msg_.timeoutTimestamp,
            payloads: new IICS26RouterMsgs.Payload[](1)
        });
        packet.payloads[0] = msg_.payload;

        commitPacket(packet);

        emit SendPacket(packet);
        return sequence;
    }

    /// @notice Receives a packet
    /// @param msg_ The message for receiving packets
    /// @inheritdoc IICS26Router
    function recvPacket(IICS26RouterMsgs.MsgRecvPacket calldata msg_) external nonReentrant {
        // TODO: Support multi-payload packets (#93)
        require(msg_.packet.payloads.length == 1, IBCMultiPayloadPacketNotSupported());
        IICS26RouterMsgs.Payload calldata payload = msg_.packet.payloads[0];

        IICS02ClientMsgs.CounterpartyInfo memory cInfo = getCounterparty(msg_.packet.destClient);
        require(
            keccak256(bytes(cInfo.clientId)) == keccak256(bytes(msg_.packet.sourceClient)),
            IBCInvalidCounterparty(cInfo.clientId, msg_.packet.sourceClient)
        );

        require(
            msg_.packet.timeoutTimestamp > block.timestamp,
            IBCInvalidTimeoutTimestamp(msg_.packet.timeoutTimestamp, block.timestamp)
        );

        bytes memory commitmentPath =
            ICS24Host.packetCommitmentPathCalldata(msg_.packet.sourceClient, msg_.packet.sequence);
        bytes32 commitmentBz = ICS24Host.packetCommitmentBytes32(msg_.packet);

        ILightClientMsgs.MsgMembership memory membershipMsg = ILightClientMsgs.MsgMembership({
            proof: msg_.proofCommitment,
            proofHeight: msg_.proofHeight,
            path: ICS24Host.prefixedPath(cInfo.merklePrefix, commitmentPath),
            value: abi.encodePacked(commitmentBz)
        });

        getClient(msg_.packet.destClient).membership(membershipMsg);

        // recvPacket will no-op if the packet receipt already exists
        bool isReceiptSet = setPacketReceipt(msg_.packet);
        if (!isReceiptSet) {
            emit Noop();
            return;
        }

        bytes memory universalErrorAck = abi.encodePacked(ICS24Host.UNIVERSAL_ERROR_ACK);
        bytes[] memory acks = new bytes[](1);
        try getIBCApp(payload.destPort).onRecvPacket(
            IIBCAppCallbacks.OnRecvPacketCallback({
                sourceClient: msg_.packet.sourceClient,
                destinationClient: msg_.packet.destClient,
                sequence: msg_.packet.sequence,
                payload: payload,
                relayer: _msgSender()
            })
        ) returns (bytes memory ack) {
            require(ack.length != 0, IBCAsyncAcknowledgementNotSupported());
            require(keccak256(ack) != keccak256(universalErrorAck), IBCErrorUniversalAcknowledgement());
            acks[0] = ack;
        } catch (bytes memory reason) {
            emit IBCAppRecvPacketCallbackError(reason);
            acks[0] = universalErrorAck;
        }

        writeAcknowledgement(msg_.packet, acks);
        emit RecvPacket(msg_.packet);
    }

    /// @notice Acknowledges a packet
    /// @param msg_ The message for acknowledging packets
    /// @inheritdoc IICS26Router
    function ackPacket(IICS26RouterMsgs.MsgAckPacket calldata msg_) external nonReentrant {
        // TODO: Support multi-payload packets #93
        require(msg_.packet.payloads.length == 1, IBCMultiPayloadPacketNotSupported());
        IICS26RouterMsgs.Payload calldata payload = msg_.packet.payloads[0];

        IICS02ClientMsgs.CounterpartyInfo memory cInfo = getCounterparty(msg_.packet.sourceClient);
        require(
            keccak256(bytes(cInfo.clientId)) == keccak256(bytes(msg_.packet.destClient)),
            IBCInvalidCounterparty(cInfo.clientId, msg_.packet.destClient)
        );

        bytes memory commitmentPath =
            ICS24Host.packetAcknowledgementCommitmentPathCalldata(msg_.packet.destClient, msg_.packet.sequence);
        bytes[] memory acks = new bytes[](1);
        acks[0] = msg_.acknowledgement;
        bytes32 commitmentBz = ICS24Host.packetAcknowledgementCommitmentBytes32(acks);

        // verify the packet acknowledgement
        ILightClientMsgs.MsgMembership memory membershipMsg = ILightClientMsgs.MsgMembership({
            proof: msg_.proofAcked,
            proofHeight: msg_.proofHeight,
            path: ICS24Host.prefixedPath(cInfo.merklePrefix, commitmentPath),
            value: abi.encodePacked(commitmentBz)
        });

        getClient(msg_.packet.sourceClient).membership(membershipMsg);

        // ackPacket will no-op if the packet commitment does not exist
        (bool isDeleted, bytes32 storedCommitment) = deletePacketCommitment(msg_.packet);
        if (!isDeleted) {
            emit Noop();
            return;
        }
        require(
            storedCommitment == ICS24Host.packetCommitmentBytes32(msg_.packet),
            IBCPacketCommitmentMismatch(storedCommitment, ICS24Host.packetCommitmentBytes32(msg_.packet))
        );

        getIBCApp(payload.sourcePort).onAcknowledgementPacket(
            IIBCAppCallbacks.OnAcknowledgementPacketCallback({
                sourceClient: msg_.packet.sourceClient,
                destinationClient: msg_.packet.destClient,
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
    function timeoutPacket(IICS26RouterMsgs.MsgTimeoutPacket calldata msg_) external nonReentrant {
        // TODO: Support multi-payload packets #93
        require(msg_.packet.payloads.length == 1, IBCMultiPayloadPacketNotSupported());
        IICS26RouterMsgs.Payload calldata payload = msg_.packet.payloads[0];

        IICS02ClientMsgs.CounterpartyInfo memory cInfo = getCounterparty(msg_.packet.sourceClient);
        require(
            keccak256(bytes(cInfo.clientId)) == keccak256(bytes(msg_.packet.destClient)),
            IBCInvalidCounterparty(cInfo.clientId, msg_.packet.destClient)
        );

        bytes memory receiptPath =
            ICS24Host.packetReceiptCommitmentPathCalldata(msg_.packet.destClient, msg_.packet.sequence);
        ILightClientMsgs.MsgMembership memory nonMembershipMsg = ILightClientMsgs.MsgMembership({
            proof: msg_.proofTimeout,
            proofHeight: msg_.proofHeight,
            path: ICS24Host.prefixedPath(cInfo.merklePrefix, receiptPath),
            value: bytes("")
        });

        uint256 counterpartyTimestamp = getClient(msg_.packet.sourceClient).membership(nonMembershipMsg);
        require(
            counterpartyTimestamp >= msg_.packet.timeoutTimestamp,
            IBCInvalidTimeoutTimestamp(msg_.packet.timeoutTimestamp, counterpartyTimestamp)
        );

        // timeoutPacket will no-op if the packet commitment does not exist
        (bool isDeleted, bytes32 storedCommitment) = deletePacketCommitment(msg_.packet);
        if (!isDeleted) {
            emit Noop();
            return;
        }
        require(
            storedCommitment == ICS24Host.packetCommitmentBytes32(msg_.packet),
            IBCPacketCommitmentMismatch(storedCommitment, ICS24Host.packetCommitmentBytes32(msg_.packet))
        );

        getIBCApp(payload.sourcePort).onTimeoutPacket(
            IIBCAppCallbacks.OnTimeoutPacketCallback({
                sourceClient: msg_.packet.sourceClient,
                destinationClient: msg_.packet.destClient,
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
    function writeAcknowledgement(IICS26RouterMsgs.Packet calldata packet, bytes[] memory acks) private {
        commitPacketAcknowledgement(packet, acks);
        emit WriteAcknowledgement(packet, acks);
    }

    /// @notice Returns the storage of the ICS26Router contract
    function _getICS26RouterStorage() private pure returns (ICS26RouterStorage storage $) {
        // solhint-disable-next-line no-inline-assembly
        assembly {
            $.slot := ICS26ROUTER_STORAGE_SLOT
        }
    }
}
