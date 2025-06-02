// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IIBCAppCallbacks } from "../msgs/IIBCAppCallbacks.sol";
import { IERC165 } from "@openzeppelin/contracts/utils/introspection/IERC165.sol";

/**
 * @title IBC Callbacks Interface
 * @notice If a contract which implements this interface sends a packet using an IBC application,
 * then it will receive callbacks for acknowledgement and timeout events.
 */
interface IIBCCallbacks is IERC165 {
    /// @notice Called when a packet acknowledgement is received by the IBC application.
    /// @param msg_ The callback message
    function onAcknowledgementPacket(IIBCAppCallbacks.OnAcknowledgementPacketCallback calldata msg_) external;

    /// @notice Called when a packet is timed out by the IBC application.
    /// @param msg_ The callback message
    function onTimeoutPacket(IIBCAppCallbacks.OnTimeoutPacketCallback calldata msg_) external;
}
