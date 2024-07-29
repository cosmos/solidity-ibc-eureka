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
 * - Prefixed denoms (source chain is not the source) and the burning of tokens related to that
 * - Separate escrow balance tracking
 * - Quite a bit of validation
 * - Receiving packets
 */
contract ICS20Transfer is IIBCApp, IICS20Errors, Ownable, ReentrancyGuard {
    string public constant ICS20_VERSION = "ics20-1";

    event LogICS20Transfer(uint256 amount, address tokenContract, address sender, string receiver);

    /// @param owner_ The owner of the contract
    constructor(address owner_) Ownable(owner_) { }

    function onSendPacket(OnSendPacketCallback calldata msg_) external override onlyOwner nonReentrant {
        if (keccak256(abi.encodePacked(msg_.packet.version)) != keccak256(abi.encodePacked(ICS20_VERSION))) {
            revert ICS20UnexpectedVersion(msg_.packet.version);
        }

        ICS20Lib.PacketData memory data = ICS20Lib.unmarshalJSON(msg_.packet.data);

        // TODO: Maybe have a "ValidateBasic" type of function that checks the packet data

        if (data.amount == 0) {
            revert ICS20InvalidAmount(data.amount);
        }

        // TODO: Handle prefixed denoms (source chain is not the source) and burn

        (address sender, bool senderConvertSuccess) = ICS20Lib.hexStringToAddress(data.sender);
        if (!senderConvertSuccess) {
            revert ICS20InvalidSender(data.sender);
        }
        if (msg_.sender != sender) {
            revert ICS20MsgSenderIsNotPacketSender(msg_.sender, sender);
        }

        (address tokenContract, bool tokenContractConvertSuccess) = ICS20Lib.hexStringToAddress(data.denom);
        if (!tokenContractConvertSuccess) {
            revert ICS20InvalidTokenContract(data.denom);
        }

        _transferFrom(sender, address(this), tokenContract, data.amount);

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

    function onAcknowledgementPacket(OnAcknowledgementPacketCallback calldata msg_)
        external
        override
        onlyOwner
        nonReentrant
    {
        if (keccak256(msg_.acknowledgement) != ICS20Lib.KECCAK256_SUCCESSFUL_ACKNOWLEDGEMENT_JSON) {
            ICS20Lib.PacketData memory data = ICS20Lib.unmarshalJSON(msg_.packet.data);
            _refundTokens(data);
        }

        // Nothing needed to be done if the acknowledgement was successful, tokens are already in escrow or burnt

        // TODO: Emit event
    }

    function onTimeoutPacket(OnTimeoutPacketCallback calldata msg_) external override onlyOwner nonReentrant {
        ICS20Lib.PacketData memory data = ICS20Lib.unmarshalJSON(msg_.packet.data);
        _refundTokens(data);
    }

    function _refundTokens(ICS20Lib.PacketData memory data) internal {
        (address tokenContract, bool tokenContractConvertSuccess) = ICS20Lib.hexStringToAddress(data.denom);
        if (!tokenContractConvertSuccess) {
            revert ICS20InvalidTokenContract(data.denom);
        }

        (address refundee, bool senderConvertSuccess) = ICS20Lib.hexStringToAddress(data.sender);
        if (!senderConvertSuccess) {
            revert ICS20InvalidSender(data.sender);
        }

        IERC20(tokenContract).safeTransfer(refundee, data.amount);
    }

    function _transferFrom(address sender, address receiver, address tokenContract, uint256 amount) internal {
        // we snapshot our current balance of this token
        uint256 ourStartingBalance = IERC20(tokenContract).balanceOf(receiver);

        IERC20(tokenContract).safeTransferFrom(sender, receiver, amount);

        // check what this particular ERC20 implementation actually gave us, since it doesn't
        // have to be at all related to the _amount
        uint256 actualEndingBalance = IERC20(tokenContract).balanceOf(receiver);

        uint256 expectedEndingBalance = ourStartingBalance + amount;
        // a very strange ERC20 may trigger this condition, if we didn't have this we would
        // underflow, so it's mostly just an error message printer
        if (actualEndingBalance <= ourStartingBalance || actualEndingBalance != expectedEndingBalance) {
            revert ICS20UnexpectedERC20Balance(expectedEndingBalance, actualEndingBalance);
        }
    }
}
