// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IICS20TransferMsgs } from "../msgs/IICS20TransferMsgs.sol";
import { IICS26RouterMsgs } from "../msgs/IICS26RouterMsgs.sol";

interface IICS20Transfer {
    /// @notice Send a transfer by constructing a message and calling IICS26Router.sendPacket
    /// @notice This function is not strictly necessary. You can construct IICS26RouterMsgs.SendPacketMsg
    /// @notice yourself and call IICS26Router.sendPacket, which uses less gas than this function
    /// @notice There is also a helper function newMsgSendPacketV1 to help construct the message
    /// @param msg_ The message for sending a transfer
    /// @return sequence The sequence number of the packet created
    function sendTransfer(IICS20TransferMsgs.SendTransferMsg calldata msg_) external returns (uint32 sequence);

    /// @notice Retrieve the escrow contract address
    /// @return The escrow contract address
    function escrow() external view returns (address);

    /// @notice Retrieve the ERC20 contract address for the given IBC denom
    /// @param denom The IBC denom
    /// @return The ERC20 contract address
    function ibcERC20Contract(string calldata denom) external view returns (address);
}
