// SPDX-License-Identifier: MIT
pragma solidity >=0.8.25;

import { ICS20Lib } from "../utils/ICS20Lib.sol";

interface IICS20TransferEvents {
    /// @notice Emitted when a packet is handled in onSendPacket and a transfer has been initiated
    /// @param packetData The transfer packet data
    /// @param erc20Address The address of the ERC20 contract of the token sent
    event ICS20Transfer(ICS20Lib.PacketDataJSON packetData, address erc20Address);

    /// @notice Emitted when a packet is received in onReceivePacket
    /// @param packetData The transfer packet data
    /// @param erc20Address The address of the ERC20 contract of the token received
    event ICS20ReceiveTransfer(ICS20Lib.PacketDataJSON packetData, address erc20Address);

    /// @notice Emitted after handling acknowledgement in onAcknowledgementPacket
    /// @param packetData The transfer packet data
    /// @param acknowledgement The acknowledgement data
    event ICS20Acknowledgement(ICS20Lib.PacketDataJSON packetData, bytes acknowledgement);

    /// @notice Emitted after handling a timeout in onTimeoutPacket
    /// @param packetData The transfer packet data
    event ICS20Timeout(ICS20Lib.PacketDataJSON packetData);
}
