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
import { IBCERC20 } from "./utils/IBCERC20.sol";

using SafeERC20 for IERC20;

/*
 * Things not handled yet:
 * - Prefixed denoms (source chain is not the source) and the burning of tokens related to that
 * - Separate escrow balance tracking
 * - Related to escrow ^: invariant checking (where to implement that?)
 * - Receiving packets
 */
contract ICS20Transfer is IIBCApp, IICS20Transfer, IICS20Errors, Ownable, ReentrancyGuard {
    mapping(string denom => IBCERC20 ibcERC20Contract) private _foreignDenomContracts;

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

    function onSendPacket(OnSendPacketCallback calldata msg_) external onlyOwner nonReentrant {
        if (keccak256(abi.encodePacked(msg_.packet.version)) != keccak256(abi.encodePacked(ICS20Lib.ICS20_VERSION))) {
            revert ICS20UnexpectedVersion(msg_.packet.version);
        }

        ICS20Lib.SendPacketData memory packetData = _unwrapSendPacketData(msg_.packet);

        // The packet sender has to be either the packet data sender or the contract itself
        // The scenarios are either the sender sent the packet directly to the router (msg_.sender == packetData.sender)
        // or sender used the sendTransfer function, which makes this contract the sender (msg_.sender == address(this))
        if (msg_.sender != packetData.sender && msg_.sender != address(this)) {
            revert ICS20MsgSenderIsNotPacketSender(msg_.sender, packetData.sender);
        }

        // transfer the tokens to us (requires the allowance to be set)
        _transferFrom(packetData.sender, address(this), packetData.erc20Contract, packetData.amount);

        // if the denom is prefixed by the port and channel on which we are sending
        // the token, then we must be returning the token back to the chain they originated from
        if (packetData.receiverChainIsSource) {
            // receiver chain is source: burn the vouchers
            // TODO: Implement escrow balance tracking (#6)
            IBCERC20 ibcERC20Contract = IBCERC20(packetData.erc20Contract);
            ibcERC20Contract.burn(packetData.amount);
        }

        emit ICS20Transfer(packetData);
    }

    function onRecvPacket(OnRecvPacketCallback calldata msg_) external onlyOwner nonReentrant returns (bytes memory) {
        // Since this function mostly returns acks, also when it fails, the ics26router (the caller) will log the ack
        if (keccak256(abi.encodePacked(msg_.packet.version)) != keccak256(abi.encodePacked(ICS20Lib.ICS20_VERSION))) {
            return ICS20Lib.errorAck(abi.encodePacked("unexpected version: ", msg_.packet.version));
        }

        (ICS20Lib.ReceivePacketData memory packetData, bytes memory err) = _unwrapReceivePacketData(msg_.packet);
        if (err.length > 0) {
            return ICS20Lib.errorAck(err);
        }

        // TODO: Implement escrow balance tracking (#6)
        if (packetData.senderChainIsSource) {
            // sender is source, so we mint vouchers
            // NOTE: The unwrap function already created a new contract if it didn't exist already
            IBCERC20(packetData.erc20Contract).mint(packetData.amount);
        }

        // transfer the tokens to the receiver
        IERC20(packetData.erc20Contract).safeTransfer(packetData.receiver, packetData.amount);

        emit ICS20ReceiveTransfer(packetData);

        return ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON;
    }

    function onAcknowledgementPacket(OnAcknowledgementPacketCallback calldata msg_) external onlyOwner nonReentrant {
        ICS20Lib.SendPacketData memory packetData = _unwrapSendPacketData(msg_.packet);
        bool isSuccessAck = true;

        if (keccak256(msg_.acknowledgement) != ICS20Lib.KECCAK256_SUCCESSFUL_ACKNOWLEDGEMENT_JSON) {
            isSuccessAck = false;
            _refundTokens(packetData);
        }

        // Nothing needed to be done if the acknowledgement was successful, tokens are already in escrow or burnt

        emit ICS20Acknowledgement(packetData, msg_.acknowledgement, isSuccessAck);
    }

    function onTimeoutPacket(OnTimeoutPacketCallback calldata msg_) external onlyOwner nonReentrant {
        ICS20Lib.SendPacketData memory packetData = _unwrapSendPacketData(msg_.packet);
        _refundTokens(packetData);

        emit ICS20Timeout(packetData);
    }

    // TODO: Implement escrow balance tracking (#6)
    function _refundTokens(ICS20Lib.SendPacketData memory data) private {
        address refundee = data.sender;
        IERC20(data.erc20Contract).safeTransfer(refundee, data.amount);
    }

    // TODO: Implement escrow balance tracking (#6)
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

    function _unwrapSendPacketData(IICS26RouterMsgs.Packet calldata packet)
        private
        view
        returns (ICS20Lib.SendPacketData memory)
    {
        ICS20Lib.PacketDataJSON memory packetData = ICS20Lib.unmarshalJSON(packet.data);
        ICS20Lib.SendPacketData memory receivePacketData = ICS20Lib.SendPacketData({
            denom: packetData.denom,
            receiverChainIsSource: false,
            erc20Contract: address(0),
            sender: address(0),
            receiver: packetData.receiver,
            amount: packetData.amount,
            memo: packetData.memo
        });

        if (packetData.amount == 0) {
            revert ICS20InvalidAmount(packetData.amount);
        }

        bool senderConvertSuccess;
        (receivePacketData.sender, senderConvertSuccess) = ICS20Lib.hexStringToAddress(packetData.sender);
        if (!senderConvertSuccess) {
            revert ICS20InvalidSender(packetData.sender);
        }

        // if the denom is prefixed by the port and channel on which we are sending the token,
        // then the receiver chain is the source of the token (i.e we need to burn when sending, or mint when refunding)
        receivePacketData.receiverChainIsSource = ICS20Lib.hasPrefix(
            bytes(packetData.denom), ICS20Lib.getDenomPrefix(packet.sourcePort, packet.sourceChannel)
        );
        if (receivePacketData.receiverChainIsSource) {
            receivePacketData.erc20Contract = address(_foreignDenomContracts[packetData.denom]);
            if (receivePacketData.erc20Contract == address(0)) {
                revert ICS20DenomNotFound(packetData.denom);
            }
        } else {
            receivePacketData.erc20Contract = address(_foreignDenomContracts[packetData.denom]);
            if (receivePacketData.erc20Contract == address(0)) {
                // this denom is not created by us, so we expect the denom to be a token contract address
                bool tokenContractConvertSuccess;
                (receivePacketData.erc20Contract, tokenContractConvertSuccess) =
                    ICS20Lib.hexStringToAddress(packetData.denom);
                if (!tokenContractConvertSuccess) {
                    revert ICS20InvalidTokenContract(packetData.denom);
                }
            }
        }

        return receivePacketData;
    }

    function _unwrapReceivePacketData(IICS26RouterMsgs.Packet calldata packet)
        private
        returns (ICS20Lib.ReceivePacketData memory, bytes memory)
    {
        ICS20Lib.PacketDataJSON memory packetData = ICS20Lib.unmarshalJSON(packet.data);
        ICS20Lib.ReceivePacketData memory receivePacketData = ICS20Lib.ReceivePacketData({
            denom: "",
            senderChainIsSource: false,
            erc20Contract: address(0),
            sender: packetData.sender,
            receiver: address(0),
            amount: packetData.amount,
            memo: packetData.memo
        });

        if (packetData.amount == 0) {
            return (receivePacketData, abi.encodePacked("invalid amount: 0"));
        }

        bool receiverConvertSuccess;
        (receivePacketData.receiver, receiverConvertSuccess) = ICS20Lib.hexStringToAddress(packetData.receiver);
        if (!receiverConvertSuccess) {
            return (receivePacketData, abi.encodePacked("invalid receiver: ", packetData.receiver));
        }

        bytes memory denomBz = bytes(packetData.denom);
        // NOTE: We use sourcePort and sourceChannel here, because the counterparty
        // chain would have prefixed with DestPort and DestChannel when originally
        // receiving this token.
        bytes memory denomPrefix = ICS20Lib.getDenomPrefix(packet.sourcePort, packet.sourceChannel);

        // if the denom is prefixed by the port and channel on which the tokens were sent (i.e. the sender chain),
        // then we are the source of the token and need to unwrap the denom trace, otherwise we add a denom trace
        if (ICS20Lib.hasPrefix(denomBz, denomPrefix)) {
            // we are the source of this token, so we unwrap and look for the token contract address
            receivePacketData.senderChainIsSource = false;
            receivePacketData.denom =
                string(ICS20Lib.slice(denomBz, denomPrefix.length, denomBz.length - denomPrefix.length));

            receivePacketData.erc20Contract = address(_foreignDenomContracts[receivePacketData.denom]);
            if (receivePacketData.erc20Contract == address(0)) {
                // this denom is not created by us, so we expect the denom to be a token contract address
                bool tokenContractConvertSuccess;
                (receivePacketData.erc20Contract, tokenContractConvertSuccess) =
                    ICS20Lib.hexStringToAddress(receivePacketData.denom);
                if (!tokenContractConvertSuccess) {
                    return (receivePacketData, abi.encodePacked("invalid token contract: ", receivePacketData.denom));
                }
            }
        } else {
            // we are not the source of this token, so we add a denom trace and find or create a token contract
            receivePacketData.senderChainIsSource = true;
            bytes memory newDenomPrefix = ICS20Lib.getDenomPrefix(packet.destPort, packet.destChannel);
            receivePacketData.denom = string(abi.encodePacked(newDenomPrefix, packetData.denom));

            // check if denom already has a contract
            receivePacketData.erc20Contract = address(_foreignDenomContracts[receivePacketData.denom]);
            if (receivePacketData.erc20Contract == address(0)) {
                // nothing exists, so we create new erc20 contract and register it in the mapping
                IBCERC20 ibcERC20 = new IBCERC20(IICS20Transfer(address(this)));
                _foreignDenomContracts[receivePacketData.denom] = ibcERC20;
                receivePacketData.erc20Contract = address(ibcERC20);
            }
        }

        return (receivePacketData, "");
    }
}
