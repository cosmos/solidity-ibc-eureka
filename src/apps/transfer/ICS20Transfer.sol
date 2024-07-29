// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.25;

import { IIBCApp } from "../../interfaces/IIBCApp.sol";
import { IICS20Errors } from "./IICS20Errors.sol";
import { ICS20Lib } from "./ICS20Lib.sol";
import { IERC20 } from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import { SafeERC20 } from "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import { Ownable } from "@openzeppelin/contracts/access/Ownable.sol";
import { ReentrancyGuard } from "@openzeppelin/contracts/utils/ReentrancyGuard.sol";

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
contract ICS20Transfer is IIBCApp, IICS20Errors, Ownable, ReentrancyGuard {
    event LogICS20Transfer(uint256 amount, address tokenContract, address sender, string receiver);

    /// @param owner_ The owner of the contract
    constructor(address owner_) Ownable(owner_) { }

    function onSendPacket(OnSendPacketCallback calldata msg_) external override onlyOwner nonReentrant {
        ICS20Lib.PacketData memory data = ICS20Lib.unmarshalJSON(msg_.packet.data);

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
        if (msg_.sender != sender) {
            revert IICS20Errors.ICS20MsgSenderIsNotPacketSender(msg_.sender, sender);
        }

        (address tokenContract, bool tokenContractConvertSuccess) = ICS20Lib.hexStringToAddress(data.denom);
        if (!tokenContractConvertSuccess) {
            revert IICS20Errors.ICS20InvalidTokenContract(data.denom);
        }

        IERC20(tokenContract).safeTransferFrom(sender, address(this), data.amount);

        // TODO: Rename and make this event better, just used for some debugging up until now
        emit LogICS20Transfer(data.amount, tokenContract, sender, data.receiver);
    }

    function onRecvPacket(OnRecvPacketCallback calldata)
        external
        override
        onlyOwner
        nonReentrant
        returns (bytes memory)
    {
        // TODO: Implement
        return "";
    }

    function onAcknowledgementPacket(OnAcknowledgementPacketCallback calldata)
        external
        override
        onlyOwner
        nonReentrant
    {
        // TODO: Implement
    }

    function onTimeoutPacket(OnTimeoutPacketCallback calldata) external override onlyOwner nonReentrant {
        // TODO: Implement
    }
}
