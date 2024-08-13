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
import { SdkCoin } from "./utils/SdkCoin.sol";
import { SafeCast } from "@openzeppelin/contracts/utils/math/SafeCast.sol";

using SafeERC20 for IERC20;

/*
 * Things not handled yet:
 * - Separate escrow balance tracking
 * - Related to escrow ^: invariant checking (where to implement that?)
 */
contract ICS20Transfer is IIBCApp, IICS20Transfer, IICS20Errors, Ownable, ReentrancyGuard {
    /// @notice Mapping of non-native denoms to their respective IBCERC20 contracts created here
    mapping(string denom => IBCERC20 ibcERC20Contract) private _foreignDenomContracts;

    /// @param owner_ The owner of the contract
    constructor(address owner_) Ownable(owner_) { }

    /// @inheritdoc IICS20Transfer
    function sendTransfer(SendTransferMsg calldata msg_) external override returns (uint32) {
        if (msg_.amount == 0) {
            revert ICS20InvalidAmount(msg_.amount);
        }

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

    /// @inheritdoc IIBCApp
    function onSendPacket(OnSendPacketCallback calldata msg_) external onlyOwner nonReentrant {
        if (keccak256(abi.encodePacked(msg_.packet.version)) != keccak256(abi.encodePacked(ICS20Lib.ICS20_VERSION))) {
            revert ICS20UnexpectedVersion(ICS20Lib.ICS20_VERSION, msg_.packet.version);
        }

        ICS20Lib.UnwrappedPacketData memory packetData = _unwrapSendPacketData(msg_.packet);

        if (packetData.amount == 0) {
            revert ICS20InvalidAmount(packetData.amount);
        }

        address sender = ICS20Lib.mustHexStringToAddress(packetData.sender);

        // The packet sender has to be either the packet data sender or the contract itself
        // The scenarios are either the sender sent the packet directly to the router (msg_.sender == packetData.sender)
        // or sender used the sendTransfer function, which makes this contract the sender (msg_.sender == address(this))
        if (msg_.sender != sender && msg_.sender != address(this)) {
            revert ICS20MsgSenderIsNotPacketSender(msg_.sender, sender);
        }

        if (!packetData.originatorChainIsSource) {
            // NOTE: Here we transfer the packetData.amount since the receiver chain is source that means that tokens
            // that
            // we are sending have arrived before from the receiving chain. The packetData.amount already took into
            // consideration the conversion, since an ERC20 contract has been created with 6 decimals.
            _transferFrom(sender, address(this), packetData.erc20Contract, packetData.amount);
            // receiver chain is source: burn the vouchers
            // TODO: Implement escrow balance tracking (#6)
            IBCERC20 ibcERC20Contract = IBCERC20(packetData.erc20Contract);
            ibcERC20Contract.burn(packetData.amount);
        } else {
            // We are sending out native EVM ERC20 tokens. We need to take into account the conversion
            // NOTE: uint64 _sdkCoinAmount returned by SdkCoin._ERC20ToSdkCoin_ConvertAmount is discarded because it
            // won't
            // be used here. Recall that the _transferFrom function requires an uint256.
            (, uint256 _remainder) = SdkCoin._ERC20ToSdkCoin_ConvertAmount(packetData.erc20Contract, packetData.amount);

            // Transfer the packetData.amount minus the remainder from the sender to this contract.
            // This step moves the correct amount of tokens, adjusted for any precision differences,
            // from the user's account to the contract's account.
            // The remainder is left in the sender's account, ensuring they aren't overcharged
            // due to any rounding or precision issues in the conversion process.
            // transfer the tokens to us (requires the allowance to be set)
            _transferFrom(sender, address(this), packetData.erc20Contract, packetData.amount - _remainder);
        }
        // DISCUSSION: do events take into account automatically the new amount?
        emit ICS20Transfer(packetData);
    }

    /// @inheritdoc IIBCApp
    function onRecvPacket(OnRecvPacketCallback calldata msg_) external onlyOwner nonReentrant returns (bytes memory) {
        // Since this function mostly returns acks, also when it fails, the ics26router (the caller) will log the ack
        if (keccak256(abi.encodePacked(msg_.packet.version)) != keccak256(abi.encodePacked(ICS20Lib.ICS20_VERSION))) {
            // TODO: Figure out if should actually error out, or if just error acking is enough
            return ICS20Lib.errorAck(abi.encodePacked("unexpected version: ", msg_.packet.version));
        }

        ICS20Lib.UnwrappedPacketData memory packetData = _unwrapReceivePacketData(msg_.packet);

        if (packetData.amount == 0) {
            return ICS20Lib.errorAck("invalid amount: 0");
        }

        (address receiver, bool receiverConvertSuccess) = ICS20Lib.hexStringToAddress(packetData.receiver);
        if (!receiverConvertSuccess) {
            return ICS20Lib.errorAck(abi.encodePacked("invalid receiver: ", packetData.receiver));
        }

        // TODO: Implement escrow balance tracking (#6)
        if (packetData.originatorChainIsSource) {
            // sender is source, so we mint vouchers
            // NOTE: The unwrap function already created a new contract with 6 decimals if it didn't exist already
            IBCERC20(packetData.erc20Contract).mint(packetData.amount);
            // transfer the tokens to the receiver
            IERC20(packetData.erc20Contract).safeTransfer(receiver, packetData.amount);
        } else {
            // receiving back tokens that were originated from native EVM tokens.
            // Use SdkCoin._SdkCoinToERC20_ConvertAmount to account for proper decimals conversions.
            (uint256 _convertedAmount) =
                SdkCoin._SdkCoinToERC20_ConvertAmount(packetData.erc20Contract, SafeCast.toUint64(packetData.amount));

            IERC20(packetData.erc20Contract).safeTransfer(receiver, _convertedAmount);
        }

        // Note the event don't take into account the conversion
        emit ICS20ReceiveTransfer(packetData);

        return ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON;
    }

    /// @inheritdoc IIBCApp
    function onAcknowledgementPacket(OnAcknowledgementPacketCallback calldata msg_) external onlyOwner nonReentrant {
        ICS20Lib.UnwrappedPacketData memory packetData = _unwrapSendPacketData(msg_.packet);

        if (keccak256(msg_.acknowledgement) != ICS20Lib.KECCAK256_SUCCESSFUL_ACKNOWLEDGEMENT_JSON) {
            _refundTokens(packetData);
        }

        // Nothing needed to be done if the acknowledgement was successful, tokens are already in escrow or burnt
        emit ICS20Acknowledgement(packetData, msg_.acknowledgement);
    }

    /// @inheritdoc IIBCApp
    function onTimeoutPacket(OnTimeoutPacketCallback calldata msg_) external onlyOwner nonReentrant {
        ICS20Lib.UnwrappedPacketData memory packetData = _unwrapSendPacketData(msg_.packet);
        _refundTokens(packetData);

        emit ICS20Timeout(packetData);
    }

    /// @notice Refund the tokens to the sender
    /// @param data The packet data
    function _refundTokens(ICS20Lib.UnwrappedPacketData memory data) private {
        address refundee = ICS20Lib.mustHexStringToAddress(data.sender);
        (, uint256 _remainder) = SdkCoin._ERC20ToSdkCoin_ConvertAmount(data.erc20Contract, data.amount);
        IERC20(data.erc20Contract).safeTransfer(refundee, data.amount - _remainder);
    }

    /// @notice Transfer tokens from sender to receiver
    /// @param sender The sender of the tokens
    /// @param receiver The receiver of the tokens
    /// @param tokenContract The address of the token contract
    /// @param amount The amount of tokens to transfer
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

    /// @notice Unwrap the packet data for sending, including finding the correct erc20 contract to use
    /// @param packet The packet to unwrap
    /// @return The unwrapped packet data
    function _unwrapSendPacketData(IICS26RouterMsgs.Packet calldata packet)
        private
        view
        returns (ICS20Lib.UnwrappedPacketData memory)
    {
        ICS20Lib.PacketDataJSON memory packetData = ICS20Lib.unmarshalJSON(packet.data);
        ICS20Lib.UnwrappedPacketData memory receivePacketData = ICS20Lib.UnwrappedPacketData({
            denom: packetData.denom,
            originatorChainIsSource: false,
            erc20Contract: address(0),
            sender: packetData.sender,
            receiver: packetData.receiver,
            amount: packetData.amount,
            memo: packetData.memo
        });

        // if the denom is NOT prefixed by the port and channel on which we are sending the token,
        // then the we are the the source of the token
        // otherwise the receiving chain is the source (i.e we need to burn when sending, or mint when refunding)
        bytes memory denomPrefix = ICS20Lib.getDenomPrefix(packet.sourcePort, packet.sourceChannel);
        receivePacketData.originatorChainIsSource = !ICS20Lib.hasPrefix(bytes(packetData.denom), denomPrefix);
        if (receivePacketData.originatorChainIsSource) {
            // we are the source of this token, so we unwrap and look for the token contract address
            receivePacketData.erc20Contract = findOrExtractERC20Address(packetData.denom);
        } else {
            // receiving chain is source of the token, so we will find the address in the mapping
            receivePacketData.erc20Contract = address(_foreignDenomContracts[packetData.denom]);
            if (receivePacketData.erc20Contract == address(0)) {
                revert ICS20DenomNotFound(packetData.denom);
            }
        }

        return receivePacketData;
    }

    /// @notice Unwrap the packet data for receiving, including finding or instantiating the erc20 contract to use
    /// @param packet The packet to unwrap
    /// @return The unwrapped packet data
    function _unwrapReceivePacketData(IICS26RouterMsgs.Packet calldata packet)
        private
        returns (ICS20Lib.UnwrappedPacketData memory)
    {
        ICS20Lib.PacketDataJSON memory packetData = ICS20Lib.unmarshalJSON(packet.data);
        ICS20Lib.UnwrappedPacketData memory receivePacketData = ICS20Lib.UnwrappedPacketData({
            denom: "",
            originatorChainIsSource: false,
            erc20Contract: address(0),
            sender: packetData.sender,
            receiver: packetData.receiver,
            amount: packetData.amount,
            memo: packetData.memo
        });

        bytes memory denomBz = bytes(packetData.denom);
        // NOTE: We use sourcePort and sourceChannel here, because the counterparty
        // chain would have prefixed with DestPort and DestChannel when originally
        // receiving this token.
        bytes memory denomPrefix = ICS20Lib.getDenomPrefix(packet.sourcePort, packet.sourceChannel);

        receivePacketData.originatorChainIsSource = !ICS20Lib.hasPrefix(denomBz, denomPrefix);

        if (receivePacketData.originatorChainIsSource) {
            // we are not the source of this token, so we add a denom trace and find or create a new token contract
            bytes memory newDenomPrefix = ICS20Lib.getDenomPrefix(packet.destPort, packet.destChannel);
            receivePacketData.denom = string(abi.encodePacked(newDenomPrefix, packetData.denom));

            receivePacketData.erc20Contract = findOrCreateERC20Address(receivePacketData.denom);
        } else {
            // we are the source of this token, so we unwrap the denom and find the token contract
            // either in the mapping or by converting the denom to an address
            receivePacketData.denom =
                string(ICS20Lib.slice(denomBz, denomPrefix.length, denomBz.length - denomPrefix.length));

            receivePacketData.erc20Contract = findOrExtractERC20Address(receivePacketData.denom);
        }

        return receivePacketData;
    }

    /// @notice Finds a contract in the foreign mapping, or expects the denom to be a token contract address
    /// @notice This function will never return address(0)
    /// @param denom The denom to find or extract the address from
    /// @return The address of the contract
    function findOrExtractERC20Address(string memory denom) internal view returns (address) {
        // check if denom already has a foreign registered contract
        address erc20Contract = address(_foreignDenomContracts[denom]);
        if (erc20Contract == address(0)) {
            // this denom is not created by us, so we expect the denom to be a token contract address
            bool convertSuccess;
            (erc20Contract, convertSuccess) = ICS20Lib.hexStringToAddress(denom);
            if (!convertSuccess) {
                revert ICS20InvalidTokenContract(denom);
            }
        }

        return erc20Contract;
    }

    /// @notice Finds a contract in the foreign mapping, or creates a new IBCERC20 contract
    /// @notice This function will never return address(0)
    /// @param denom The denom to find or create the contract for
    /// @return The address of the erc20 contract
    function findOrCreateERC20Address(string memory denom) internal returns (address) {
        // check if denom already has a foreign registered contract
        address erc20Contract = address(_foreignDenomContracts[denom]);
        if (erc20Contract == address(0)) {
            // nothing exists, so we create new erc20 contract and register it in the mapping
            IBCERC20 ibcERC20 = new IBCERC20(IICS20Transfer(address(this)));
            _foreignDenomContracts[denom] = ibcERC20;
            erc20Contract = address(ibcERC20);
        }

        return erc20Contract;
    }
}
