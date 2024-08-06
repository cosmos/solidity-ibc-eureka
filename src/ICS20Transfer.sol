// SPDX-License-Identifier: MIT
pragma solidity >=0.8.25;

import { IIBCApp } from "./interfaces/IIBCApp.sol";
import { IICS20Errors } from "./errors/IICS20Errors.sol";
import { ICS20Lib } from "./utils/ICS20Lib.sol";
import { IERC20 } from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import { SafeERC20 } from "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import { Ownable } from "@openzeppelin/contracts/access/Ownable.sol";
import { ReentrancyGuard } from "@openzeppelin/contracts/utils/ReentrancyGuard.sol";
import { IICS20Transfer } from "./interfaces/IICS20Transfer.sol";
import { IICS26Router } from "./interfaces/IICS26Router.sol";
import { IICS26RouterMsgs } from "./msgs/IICS26RouterMsgs.sol";
import { Strings } from "@openzeppelin/contracts/utils/Strings.sol";

using SafeERC20 for IERC20;

/*
 * Things not handled yet:
 * - Prefixed denoms (source chain is not the source) and the burning of tokens related to that
 * - Separate escrow balance tracking
 * - Related to escrow ^: invariant checking (where to implement that?)
 * - Receiving packets
 */
contract ICS20Transfer is IIBCApp, IICS20Transfer, IICS20Errors, Ownable, ReentrancyGuard {
    /// @param owner_ The owner of the contract
    constructor(address owner_) Ownable(owner_) { }

    function sendTransfer(SendTransferMsg calldata msg_) external override returns (uint32) {
        IICS26Router ibcRouter = IICS26Router(owner());

        string memory sender = Strings.toHexString(msg.sender);
        string memory sourcePort = "transfer"; // TODO: Find a way to figure out the source port
        bytes memory packetData;
        if (bytes(msg_.memo).length == 0) {
            packetData = ICS20Lib.marshalJSON(msg_.denom, msg_.amount, sender, msg_.receiver);
        } else {
            packetData = ICS20Lib.marshalJSON(msg_.denom, msg_.amount, sender, msg_.receiver, msg_.memo);
        }

        IICS26RouterMsgs.MsgSendPacket memory msgSendPacket = IICS26RouterMsgs.MsgSendPacket({
            sourcePort: sourcePort,
            sourceChannel: msg_.sourceChannel,
            destPort: msg_.destPort,
            data: packetData,
            timeoutTimestamp: msg_.timeoutTimestamp, // TODO: Default timestamp?
            version: ICS20Lib.ICS20_VERSION
        });

        return ibcRouter.sendPacket(msgSendPacket);
    }

    function onSendPacket(OnSendPacketCallback calldata msg_) external override onlyOwner nonReentrant {
        if (keccak256(abi.encodePacked(msg_.packet.version)) != keccak256(abi.encodePacked(ICS20Lib.ICS20_VERSION))) {
            revert ICS20UnexpectedVersion(msg_.packet.version);
        }

        ICS20Lib.UnwrappedFungibleTokenPacketData memory packetData = ICS20Lib.unwrapPacketData(msg_.packet.data);

        // TODO: Maybe have a "ValidateBasic" type of function that checks the packet data, could be done in unwrapping?

        if (packetData.amount == 0) {
            revert ICS20InvalidAmount(packetData.amount);
        }

        // TODO: Handle prefixed denoms (source chain is not the source) and burn

        // The packet sender has to be either the packet data sender or the contract itself
        // The scenarios are either the sender sent the packet directly to the router (msg_.sender == packetData.sender)
        // or sender used the sendTransfer function, which makes this contract the sender (msg_.sender == address(this))
        if (msg_.sender != packetData.sender && msg_.sender != address(this)) {
            revert ICS20MsgSenderIsNotPacketSender(msg_.sender, packetData.sender);
        }

        _transferFrom(packetData.sender, address(this), packetData.erc20ContractAddress, packetData.amount);

        emit ICS20Transfer(packetData);
    }

    function onRecvPacket(OnRecvPacketCallback calldata msg_)
        external
        override
        onlyOwner
        nonReentrant
        returns (bytes memory)
    {
        // TODO Emit error event
        if (keccak256(abi.encodePacked(msg_.packet.version)) != keccak256(abi.encodePacked(ICS20Lib.ICS20_VERSION))) {
            return ICS20Lib.errorAck(abi.encodePacked("unexpected version: ", msg_.packet.version));
        }

        ICS20Lib.PacketDataJSON memory packetData = ICS20Lib.unmarshalJSON(msg_.packet.data);
        if (packetData.amount == 0) {
            return ICS20Lib.errorAck(abi.encodePacked("invalid amount: 0"));
        }

        (address receiver, bool receiverConvertSuccess) = ICS20Lib.hexStringToAddress(packetData.receiver);
        if (!receiverConvertSuccess) {
            return ICS20Lib.errorAck(abi.encodePacked("invalid receiver: ", packetData.receiver));
        }

        // TODO: Handle non-contract denoms (destination chain is not source)
        bytes memory denomPrefix = ICS20Lib.getDenomPrefix(msg_.packet.sourcePort, msg_.packet.sourceChannel);
        bytes memory denom = bytes(packetData.denom);
        if (
            denom.length >= denomPrefix.length
                && ICS20Lib.equal(ICS20Lib.slice(denom, 0, denomPrefix.length), denomPrefix)
        ) {
            // sender chain is not the source, unescrow tokens
            // TODO: Implement escrow balance tracking

            string memory unprefixedDenom =
                string(ICS20Lib.slice(denom, denomPrefix.length, denom.length - denomPrefix.length));
            (address tokenContract, bool tokenContractConvertSuccess) = ICS20Lib.hexStringToAddress(unprefixedDenom);
            if (!tokenContractConvertSuccess) {
                return ICS20Lib.errorAck(abi.encodePacked("invalid token contract: ", unprefixedDenom));
            }

            IERC20(tokenContract).safeTransfer(receiver, packetData.amount);
        } else {
            // sender chain is the source, mint vouchers
            // TODO: Implement escrow balance tracking
            // TODO: Implement creating (new erc20 contracts), looking up and minting of vouchers
            // solhint-disable-next-line
            revert("not supported: sender denom is source");
        }

        emit ICS20ReceiveTransfer(packetData);

        return ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON;
    }

    function onAcknowledgementPacket(OnAcknowledgementPacketCallback calldata msg_)
        external
        override
        onlyOwner
        nonReentrant
    {
        ICS20Lib.UnwrappedFungibleTokenPacketData memory packetData = ICS20Lib.unwrapPacketData(msg_.packet.data);
        bool isSuccessAck = true;

        if (keccak256(msg_.acknowledgement) != ICS20Lib.KECCAK256_SUCCESSFUL_ACKNOWLEDGEMENT_JSON) {
            isSuccessAck = false;
            _refundTokens(packetData);
        }

        // Nothing needed to be done if the acknowledgement was successful, tokens are already in escrow or burnt

        emit ICS20Acknowledgement(packetData, msg_.acknowledgement, isSuccessAck);
    }

    function onTimeoutPacket(OnTimeoutPacketCallback calldata msg_) external override onlyOwner nonReentrant {
        ICS20Lib.UnwrappedFungibleTokenPacketData memory packetData = ICS20Lib.unwrapPacketData(msg_.packet.data);
        _refundTokens(packetData);

        emit ICS20Timeout(packetData);
    }

    // TODO: Implement escrow balance tracking
    function _refundTokens(ICS20Lib.UnwrappedFungibleTokenPacketData memory data) private {
        address refundee = data.sender;
        IERC20(data.erc20ContractAddress).safeTransfer(refundee, data.amount);
    }

    // TODO: Implement escrow balance tracking
    function _transferFrom(address sender, address receiver, address tokenContract, uint256 amount) private {
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
