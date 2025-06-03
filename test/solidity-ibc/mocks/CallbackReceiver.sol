// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

// solhint-disable no-empty-blocks

import { IIBCCallbacks } from "../../../contracts/interfaces/IIBCCallbacks.sol";
import { IIBCAppCallbacks } from "../../../contracts/msgs/IIBCAppCallbacks.sol";
import { IBCCallbackReceiver } from "../../../contracts/utils/IBCCallbackReceiver.sol";

/// @title CallbackReceiver
/// @notice A contract that implements the IIBCCallbacks interface to receive callbacks from IBC applications.
contract CallbackReceiver is IBCCallbackReceiver {
    /// @inheritdoc IIBCCallbacks
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

    /// @inheritdoc IIBCCallbacks
    function onTimeoutPacket(IIBCAppCallbacks.OnTimeoutPacketCallback calldata msg_) external override {
        // Handle the timeout logic here
        // For example, emit an event or update state
    }
}
