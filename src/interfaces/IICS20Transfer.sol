// SPDX-License-Identifier: MIT
pragma solidity >=0.8.25;

import { IICS20TransferMsgs } from "../msgs/IICS20TransferMsgs.sol";
import { IICS20TransferEvents } from "../events/IICS20TransferEvents.sol";

interface IICS20Transfer is IICS20TransferMsgs, IICS20TransferEvents {

    /// @notice Send a transfer
    /// @param msg The message for sending a transfer
    /// @return sequence The sequence number of the packet created
    function sendTransfer(SendTransferMsg calldata msg) external returns (uint32 sequence);
}
