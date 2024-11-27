// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { ICS20Lib } from "../utils/ICS20Lib.sol";
import { IICS20TransferMsgs } from "../msgs/IICS20TransferMsgs.sol";
import { IICS26RouterMsgs } from "../msgs/IICS26RouterMsgs.sol";

interface IICS20Transfer is IICS20TransferMsgs {
    /// @notice Send a transfer by constructing a message and calling IICS26Router.sendPacket
    /// @notice This function is not strictly necessary. You can construct IICS26RouterMsgs.SendPacketMsg
    /// @notice yourself and call IICS26Router.sendPacket, which uses less gas than this function
    /// @notice There is also a helper function newMsgSendPacketV1 to help construct the message
    /// @param msg The message for sending a transfer
    /// @return sequence The sequence number of the packet created
    function sendTransfer(SendTransferMsg calldata msg) external returns (uint32 sequence);

    /// @notice Retrieve the escrow contract address
    /// @return The escrow contract address
    function escrow() external view returns (address);

    /// @notice Create an ICS26RouterMsgs.MsgSendPacket message for ics20-1.
    /// @notice This is a helper function for constructing the MsgSendPacket for ICS26Router.
    /// @param sender The sender of the transfer
    /// @param msg The message for sending a transfer
    /// @return The constructed MsgSendPacket
    function newMsgSendPacketV1(
        address sender,
        SendTransferMsg calldata msg
    )
        external
        view
        returns (IICS26RouterMsgs.MsgSendPacket memory);
}
