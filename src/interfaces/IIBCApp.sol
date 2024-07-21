// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.25;

import { IIBCAppCallbacks } from "../msgs/IIBCAppCallbacks.sol";

interface IIBCApp is IIBCAppCallbacks {
    // @notice Called when a packet is to be sent to the counterparty chain.
    // @param msg The callback message
    function onSendPacket(OnSendPacketCallback calldata msg) external;

    // @notice Called when a packet is received from the counterparty chain.
    // @param msg The callback message
    // @return The acknowledgement data
    function onRecvPacket(OnRecvPacketCallback calldata msg) external returns (bytes memory);

    // @notice Called when a packet acknowledgement is received from the counterparty chain.
    // @param msg The callback message
    function onAcknowledgementPacket(OnAcknowledgementPacketCallback calldata msg) external;

    // @notice Called when a packet is timed out.
    // @param msg The callback message
    function onTimeoutPacket(OnTimeoutPacketCallback calldata msg) external;
}
