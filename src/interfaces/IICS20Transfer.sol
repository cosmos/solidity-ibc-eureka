// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { ICS20Lib } from "../utils/ICS20Lib.sol";
import { IICS20TransferMsgs } from "../msgs/IICS20TransferMsgs.sol";
import { IICS26RouterMsgs } from "../msgs/IICS26RouterMsgs.sol";

interface IICS20Transfer is IICS20TransferMsgs {
    /// @notice Called when a packet is handled in onSendPacket and a transfer has been initiated
    /// @param packetData The transfer packet data
    /// @param erc20Address The address of the ERC20 contract of the token sent
    event ICS20Transfer(ICS20Lib.PacketDataJSON packetData, address erc20Address);

    /// @notice Called when a packet is received in onReceivePacket
    /// @param packetData The transfer packet data
    /// @param erc20Address The address of the ERC20 contract of the token received
    event ICS20ReceiveTransfer(ICS20Lib.PacketDataJSON packetData, address erc20Address);

    /// @notice Called after handling acknowledgement in onAcknowledgementPacket
    /// @param packetData The transfer packet data
    /// @param acknowledgement The acknowledgement data
    event ICS20Acknowledgement(ICS20Lib.PacketDataJSON packetData, bytes acknowledgement);

    /// @notice Called after handling a timeout in onTimeoutPacket
    /// @param packetData The transfer packet data
    event ICS20Timeout(ICS20Lib.PacketDataJSON packetData);

    /// @notice Send a transfer by constructing a message and calling IICS26Router.sendPacket
    /// @notice This function is not strictly necessary. You can construct IICS26RouterMsgs.SendPacketMsg
    /// @notice yourself and call IICS26Router.sendPacket, which uses less gas than this function
    /// @notice There is also a helper function createMsgSendPacket to help construct the message
    /// @param msg The message for sending a transfer
    /// @return sequence The sequence number of the packet created
    function sendTransfer(SendTransferMsg calldata msg) external returns (uint32 sequence);

    /// @notice Retrieve the escrow contract address
    /// @return The escrow contract address
    function escrow() external view returns (address);

    /// @notice Create an ICS26RouterMsgs.MsgSendPacket message for sending a transfer.
    /// @notice This is a helper function for constructing the MsgSendPacket for ICS26Router.
    /// @param denom ERC20 address of the token to be transferred
    /// @param amount Amount of tokens to be transferred
    /// @param sender Sender of the tokens
    /// @param receiver Receiver of the tokens
    /// @param sourceChannel Source channel of the packet
    /// @param destPort Destination port of the packet
    /// @param timeoutTimestamp Timeout timestamp of the packet
    /// @param memo Optional memo
    /// @return The constructed MsgSendPacket
    function createMsgSendPacket(
        string calldata denom,
        uint256 amount,
        address sender,
        string calldata receiver,
        string calldata sourceChannel,
        string calldata destPort,
        uint64 timeoutTimestamp,
        string calldata memo
    )
        external
        view
        returns (IICS26RouterMsgs.MsgSendPacket memory);
}
