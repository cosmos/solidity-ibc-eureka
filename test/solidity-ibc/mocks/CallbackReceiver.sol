// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

// solhint-disable no-empty-blocks

import { IIBCCallbacks } from "../../../contracts/interfaces/IIBCCallbacks.sol";
import { IIBCAppCallbacks } from "../../../contracts/msgs/IIBCAppCallbacks.sol";
import { ERC165 } from "@openzeppelin-contracts/utils/introspection/ERC165.sol";

/// @title CallbackReceiver
/// @notice A contract that implements the IIBCCallbacks interface to receive callbacks from IBC applications.
contract CallbackReceiver is IIBCCallbacks, ERC165 {
    /// @notice Called when a packet acknowledgement is received by the IBC application.
    /// @param success Whether the packet was successfully received by the destination chain
    /// @param msg_ The callback message
    function onAckPacket(
        bool success,
        IIBCAppCallbacks.OnAcknowledgementPacketCallback calldata msg_
    )
        external
        override
    {
        // Handle the acknowledgement logic here
        // For example, emit an event or update state
    }

    /// @notice Called when a packet is timed out by the IBC application.
    /// @param msg_ The callback message
    function onTimeoutPacket(IIBCAppCallbacks.OnTimeoutPacketCallback calldata msg_) external override {
        // Handle the timeout logic here
        // For example, emit an event or update state
    }

    /// @inheritdoc ERC165
    /// @dev This function signals that this contract supports the IIBCCallbacks interface.
    function supportsInterface(bytes4 interfaceId) public view override(ERC165) returns (bool) {
        return interfaceId == type(IIBCCallbacks).interfaceId || super.supportsInterface(interfaceId);
    }
}
