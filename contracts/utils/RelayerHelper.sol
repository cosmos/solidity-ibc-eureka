// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IICS26RouterMsgs } from "../msgs/IICS26RouterMsgs.sol";

import { IICS26Router } from "../interfaces/IICS26Router.sol";
import { IIBCStore } from "../interfaces/IIBCStore.sol";
import { IRelayerHelper } from "../interfaces/IRelayerHelper.sol";

import { ICS24Host } from "./ICS24Host.sol";
import { Multicall } from "@openzeppelin/contracts/utils/Multicall.sol";

/// @title Relayer Helper
/// @notice RelayerHelper is a helper contract for relayers, providing utility queries and multicall functions
contract RelayerHelper is IRelayerHelper, Multicall {
    /// @notice The ICS26 router
    address public immutable ics26Router;

    constructor(address _ics26Router) {
        ics26Router = _ics26Router;
    }

    /// @inheritdoc IRelayerHelper
    function recvPacketWithGasLimit(IICS26RouterMsgs.MsgRecvPacket calldata msg_, uint256 gasLimit) external {
        IICS26Router(ics26Router).recvPacket{gas: gasLimit}(msg_);
    }

    /// @inheritdoc IRelayerHelper
    function ackPacketWithGasLimit(IICS26RouterMsgs.MsgAckPacket calldata msg_, uint256 gasLimit) external {
        IICS26Router(ics26Router).ackPacket{gas: gasLimit}(msg_);
    }

    /// @inheritdoc IRelayerHelper
    function timeoutPacketWithGasLimit(IICS26RouterMsgs.MsgTimeoutPacket calldata msg_, uint256 gasLimit) external {
        IICS26Router(ics26Router).timeoutPacket{gas: gasLimit}(msg_);
    }

    /// @inheritdoc IRelayerHelper
    function isPacketReceived(IICS26RouterMsgs.Packet calldata packet) public view returns (bool) {
        bytes32 expReceipt = ICS24Host.packetReceiptCommitmentBytes32(packet);
        return expReceipt == queryPacketReceipt(packet.destClient, packet.sequence);
    }

    /// @inheritdoc IRelayerHelper
    function isPacketReceiveSuccessful(IICS26RouterMsgs.Packet calldata packet) external view returns (bool) {
        if (!isPacketReceived(packet)) {
            return false;
        }

        bytes[] memory errorAck = new bytes[](1);
        errorAck[0] = ICS24Host.UNIVERSAL_ERROR_ACK;
        bytes32 errorAckCommitment = ICS24Host.packetAcknowledgementCommitmentBytes32(errorAck);
        bytes32 storedAckCommitment = queryAckCommitment(packet.destClient, packet.sequence);
        return storedAckCommitment != 0 && storedAckCommitment != errorAckCommitment;
    }

    /// @inheritdoc IRelayerHelper
    function queryPacketReceipt(string calldata clientId, uint64 sequence) public view returns (bytes32) {
        bytes32 path = ICS24Host.packetReceiptCommitmentKeyCalldata(clientId, sequence);
        return IIBCStore(ics26Router).getCommitment(path);
    }

    /// @inheritdoc IRelayerHelper
    function queryPacketCommitment(string calldata clientId, uint64 sequence) public view returns (bytes32) {
        bytes32 path = ICS24Host.packetCommitmentKeyCalldata(clientId, sequence);
        return IIBCStore(ics26Router).getCommitment(path);
    }

    /// @inheritdoc IRelayerHelper
    function queryAckCommitment(string calldata clientId, uint64 sequence) public view returns (bytes32) {
        bytes32 path = ICS24Host.packetAcknowledgementCommitmentKeyCalldata(clientId, sequence);
        return IIBCStore(ics26Router).getCommitment(path);
    }
}
