// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IIBCAppCallbacks } from "../msgs/IIBCAppCallbacks.sol";
import { IIBCCallbacks } from "../interfaces/IIBCCallbacks.sol";

import { ERC165Checker } from "@openzeppelin/contracts/utils/introspection/ERC165Checker.sol";

/// @title IBC Callbacks Library
/// @notice This library provides utility functions for IBC Apps to make callbacks to sender contracts.
library IBCCallbacksLib {
    /// @notice Checks if the given address implements the IIBCCallbacks interface.
    /// @param sender The address to check
    /// @return bool True if the address implements IIBCCallbacks, false otherwise
    function _supportsCallbacks(address sender) private view returns (bool) {
        return ERC165Checker.supportsInterface(sender, type(IIBCCallbacks).interfaceId);
    }

    /// @notice Make a callback to the sender contract when a packet is acknowledged if it supports IIBCCallbacks.
    /// @param callbackAddress The address of the callback contract
    /// @param success Whether the packet was successfully received by the destination chain
    /// @param msg_ The callback message containing details about the acknowledgement
    function ackPacketCallback(
        address callbackAddress,
        bool success,
        IIBCAppCallbacks.OnAcknowledgementPacketCallback calldata msg_
    )
        internal
    {
        if (_supportsCallbacks(callbackAddress)) {
            IIBCCallbacks(callbackAddress).onAckPacket(success, msg_);
        }
    }

    /// @notice Make a callback to the sender contract when a packet times out if it supports IIBCCallbacks.
    /// @param callbackAddress The address of the callback contract
    /// @param msg_ The callback message containing details about the timeout
    function timeoutPacketCallback(
        address callbackAddress,
        IIBCAppCallbacks.OnTimeoutPacketCallback calldata msg_
    )
        internal
    {
        if (_supportsCallbacks(callbackAddress)) {
            IIBCCallbacks(callbackAddress).onTimeoutPacket(msg_);
        }
    }
}
