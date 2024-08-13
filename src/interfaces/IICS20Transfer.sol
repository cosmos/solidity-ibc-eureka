// SPDX-License-Identifier: MIT
pragma solidity >=0.8.25;

import { ICS20Lib } from "../utils/ICS20Lib.sol";
import { IICS20TransferMsgs } from "../msgs/IICS20TransferMsgs.sol";

interface IICS20Transfer is IICS20TransferMsgs {
    /// @notice Called when a packet is handled in onSendPacket and a transfer has been initiated
    /// @param packetData The transfer packet data
    event ICS20Transfer(ICS20Lib.UnwrappedPacketData packetData);

    /// @notice Called when a packet is received in onReceivePacket
    /// @param packetData The transfer packet data
    event ICS20ReceiveTransfer(ICS20Lib.UnwrappedPacketData packetData);

    /// @notice Called after handling acknowledgement in onAcknowledgementPacket
    /// @param packetData The transfer packet data
    /// @param acknowledgement The acknowledgement data
    event ICS20Acknowledgement(ICS20Lib.UnwrappedPacketData packetData, bytes acknowledgement);

    /// @notice Called after handling a timeout in onTimeoutPacket
    /// @param packetData The transfer packet data
    event ICS20Timeout(ICS20Lib.UnwrappedPacketData packetData);

    /// @notice Send a transfer
    /// @param msg The message for sending a transfer
    /// @return sequence The sequence number of the packet created
    function sendTransfer(SendTransferMsg calldata msg) external returns (uint32 sequence);
}
