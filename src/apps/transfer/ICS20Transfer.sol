// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.25;

import {IIBCApp} from "../../interfaces/IIBCApp.sol";
import "./ICS20Lib.sol";
import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";

using SafeERC20 for IERC20;

/*
 * Things not handled yet:
 * - permission control (anyone can currently call the functions)
 * - Prefixed denoms (source chain is not the source) and the burning of tokens related to that
 * - Separate escrow balance tracking
 * - Quite a bit of validation
 * - Receiving packets
 * - Acknowledgement and timeout handling
 */
contract ICS20Transfer is IIBCApp {
    event LogICS20Transfer(uint256 amount, address tokenContract, address sender, string receiver);

    function onSendPacket(OnSendPacketCallback calldata msg) external override {
        ICS20Lib.PacketData memory data = ICS20Lib.unmarshalJSON(msg.packet.data);

        // TODO: Verify version
        // TODO: Maybe have a "ValidateBasic" type of function that checks the packet data

        if (data.amount == 0) {
            revert IICS20Errors.ICS20InvalidAmount(data.amount);
        }

        // TODO: Handle prefixed denoms (source chain is not the source) and burn

        (address sender, bool senderConvertSuccess) = ICS20Lib.hexStringToAddress(data.sender);
        if (!senderConvertSuccess) {
            revert IICS20Errors.ICS20InvalidSender(data.sender);
        }
        if (msg.sender != sender) {
            revert IICS20Errors.ICS20MsgSenderIsNotPacketSender(msg.sender, sender);
        }

        (address tokenContract, bool tokenContractConvertSuccess) = ICS20Lib.hexStringToAddress(data.denom);
        if (!tokenContractConvertSuccess) {
            revert IICS20Errors.ICS20InvalidTokenContract(data.denom);
        }

        IERC20(tokenContract).safeTransferFrom(sender, address(this), data.amount);

        // TODO: Rename and make this event better, just used for some debugging up until now
        emit LogICS20Transfer(data.amount, tokenContract, sender, data.receiver);
    }

    function onRecvPacket(OnRecvPacketCallback calldata msg) external override returns (bytes memory) {
        revert("not implemented");
    }

    function onAcknowledgementPacket(OnAcknowledgementPacketCallback calldata msg) external override {
        revert("not implemented");
    }

    function onTimeoutPacket(OnTimeoutPacketCallback calldata msg) external override {
        revert("not implemented");
    }
}