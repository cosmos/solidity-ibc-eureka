// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IICS27GMPMsgs } from "../msgs/IICS27GMPMsgs.sol";

interface IICS27GMP {
    /// @notice Send a GMP packet by calling IICS26Router.sendPacket
    /// @param msg_ The message for sending a GMP packet
    /// @return sequence The sequence number of the packet created
    function sendCall(IICS27GMPMsgs.SendCallMsg calldata msg_) external returns (uint64 sequence);
}
