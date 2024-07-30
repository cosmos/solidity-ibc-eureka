// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.25;

import { IIBCApp } from "../../interfaces/IIBCApp.sol";
import { IICS20Errors } from "./IICS20Errors.sol";
import { ICS20Lib } from "./ICS20Lib.sol";
import { IERC20 } from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import { SafeERC20 } from "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import { Ownable } from "@openzeppelin/contracts/access/Ownable.sol";
import { ReentrancyGuard } from "@openzeppelin/contracts/utils/ReentrancyGuard.sol";
import { IICS20Transfer } from "./IICS20Transfer.sol";

using SafeERC20 for IERC20;

/*
 * Things not handled yet:
 * - Prefixed denoms (source chain is not the source) and the burning of tokens related to that
 * - Separate escrow balance tracking
 * - Related to escrow ^: invariant checking (where to implement that)?
 * - Receiving packets
 */
contract ICS20Transfer is IIBCApp, IICS20Transfer, IICS20Errors, Ownable, ReentrancyGuard {
    string public constant ICS20_VERSION = "ics20-1";

    /// @param owner_ The owner of the contract
    constructor(address owner_) Ownable(owner_) { }

    function onSendPacket(OnSendPacketCallback calldata msg_) external override onlyOwner nonReentrant {
        if (keccak256(abi.encodePacked(msg_.packet.version)) != keccak256(abi.encodePacked(ICS20_VERSION))) {
            revert ICS20UnexpectedVersion(msg_.packet.version);
        }

        UnwrappedFungibleTokenPacketData memory data = _unwrapPacketData(msg_.packet.data);

        // TODO: Maybe have a "ValidateBasic" type of function that checks the packet data, could be done in unwrapping?

        if (data.amount == 0) {
            revert ICS20InvalidAmount(data.amount);
        }

        // TODO: Handle prefixed denoms (source chain is not the source) and burn

        if (msg_.sender != data.sender) {
            revert ICS20MsgSenderIsNotPacketSender(msg_.sender, data.sender);
        }

        _transferFrom(data.sender, address(this), data.erc20ContractAddress, data.amount);

        emit ICS20Transfer(data.sender, data.receiver, data.erc20ContractAddress, data.amount, data.memo);
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
        UnwrappedFungibleTokenPacketData memory data = _unwrapPacketData(msg_.packet.data);
        bool isSuccessAck = true;

        if (keccak256(msg_.acknowledgement) != ICS20Lib.KECCAK256_SUCCESSFUL_ACKNOWLEDGEMENT_JSON) {
            isSuccessAck = false;
            _refundTokens(data);
        }

        // Nothing needed to be done if the acknowledgement was successful, tokens are already in escrow or burnt

        emit ICS20Acknowledgement(
            data.sender,
            data.receiver,
            data.erc20ContractAddress,
            data.amount,
            data.memo,
            msg_.acknowledgement,
            isSuccessAck
        );
    }

    function onTimeoutPacket(OnTimeoutPacketCallback calldata msg_) external override onlyOwner nonReentrant {
        UnwrappedFungibleTokenPacketData memory data = _unwrapPacketData(msg_.packet.data);
        _refundTokens(data);

        emit ICS20Timeout(data.sender, data.erc20ContractAddress, data.memo);
    }

    function _refundTokens(UnwrappedFungibleTokenPacketData memory data) internal {
        address refundee = data.sender;
        IERC20(data.erc20ContractAddress).safeTransfer(refundee, data.amount);
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

    function _unwrapPacketData(bytes calldata data) internal pure returns (UnwrappedFungibleTokenPacketData memory) {
        ICS20Lib.PacketDataJSON memory packetData = ICS20Lib.unmarshalJSON(data);

        (address tokenContract, bool tokenContractConvertSuccess) = ICS20Lib.hexStringToAddress(packetData.denom);
        if (!tokenContractConvertSuccess) {
            revert ICS20InvalidTokenContract(packetData.denom);
        }

        (address sender, bool senderConvertSuccess) = ICS20Lib.hexStringToAddress(packetData.sender);
        if (!senderConvertSuccess) {
            revert ICS20InvalidSender(packetData.sender);
        }

        return UnwrappedFungibleTokenPacketData({
            erc20ContractAddress: tokenContract,
            amount: packetData.amount,
            sender: sender,
            receiver: packetData.receiver,
            memo: packetData.memo
        });
    }
}
