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
contract SdkICS20Transfer is IIBCApp, IICS20Transfer, IICS20Errors, Ownable, ReentrancyGuard {
    /// @notice Mapping of non-native denoms to their respective IBCERC20 contracts created here
    mapping(string denom => IBCERC20 ibcERC20Contract) private _foreignDenomContracts;

    /// @param owner_ The owner of the contract
    constructor(address owner_) Ownable(owner_) { }

    /// @inheritdoc IICS20Transfer
    function sendTransfer(SendTransferMsg calldata msg_) external override returns (uint32) {
        if (msg_.amount == 0) {
            revert ICS20InvalidAmount(msg_.amount);
        }

        // we expect the denom to be an erc20 address
        address contractAddress = ICS20Lib.mustHexStringToAddress(msg_.denom);

        string memory fullDenomPath;
        try IBCERC20(contractAddress).fullDenomPath() returns (string memory ibcERC20FullDenomPath) {
            // if the address is one of our IBCERC20 contracts, we get the correct denom for the packet there
            fullDenomPath = ibcERC20FullDenomPath;
        } catch {
            // otherwise this is just an ERC20 address, so we use it as the denom
            fullDenomPath = msg_.denom;
        }

        // Use the _sdkCoinAmount to populate the packetData with a uint256 representation of the uint64 supported
        // in the cosmos world that consider the proper decimals conversions.
        // Since we just transfer the converted amount, we discard the remainder as it stays in the users account
        (uint64 _sdkCoinAmount,) = SdkCoin.convertAmountERC20ToSdkCoin(contractAddress, msg_.amount);

        if (_sdkCoinAmount == 0) {
            revert ICS20InvalidAmountAfterConversion(msg_.amount, _sdkCoinAmount);
        }

        bytes memory packetData = ICS20Lib.marshalJSON(
            fullDenomPath, _sdkCoinAmount, Strings.toHexString(msg.sender), msg_.receiver, msg_.memo
        );
        IICS26RouterMsgs.MsgSendPacket memory msgSendPacket = IICS26RouterMsgs.MsgSendPacket({
            sourcePort: ICS20Lib.DEFAULT_PORT_ID,
            sourceChannel: msg_.sourceChannel,
            destPort: msg_.destPort,
            data: packetData,
            timeoutTimestamp: msg_.timeoutTimestamp, // TODO: Default timestamp?
            version: ICS20Lib.ICS20_VERSION
        });

        return IICS26Router(owner()).sendPacket(msgSendPacket);
    }

    /// @inheritdoc IIBCApp
    function onSendPacket(OnSendPacketCallback calldata msg_) external onlyOwner nonReentrant {
        if (keccak256(abi.encodePacked(msg_.packet.version)) != keccak256(abi.encodePacked(ICS20Lib.ICS20_VERSION))) {
            revert ICS20UnexpectedVersion(ICS20Lib.ICS20_VERSION, msg_.packet.version);
        }

        ICS20Lib.PacketDataJSON memory packetData = ICS20Lib.unmarshalJSON(msg_.packet.data);

        if (packetData.amount == 0) {
            revert ICS20InvalidAmount(packetData.amount);
        }

        address sender = ICS20Lib.mustHexStringToAddress(packetData.sender);

        // The packet sender has to be the contract itself.
        // Because of the packetData massaging we do in sendTransfer to convert the amount to sdkCoin, we don't allow
        // this function to be called by anyone else. They could end up transferring a larger amount than intended.
        if (msg_.sender != address(this)) {
            revert ICS20UnauthorizedPacketSender(msg_.sender);
        }

        (address erc20Address, bool originatorChainIsSource) = getSendERC20AddressAndSource(msg_.packet, packetData);

        // Use SdkCoin.convertAmountSdkCoinToERC20 to consider the proper decimals conversions.
        uint256 _convertedAmount =
            SdkCoin.convertAmountSdkCoinToERC20(erc20Address, SafeCast.toUint64(packetData.amount));

        // transfer the tokens to us (requires the allowance to be set)
        _transferFrom(sender, address(this), erc20Address, _convertedAmount);

        if (!originatorChainIsSource) {
            // receiver chain is source: burn the vouchers
            // TODO: Implement escrow balance tracking (#6)
            IBCERC20 ibcERC20Contract = IBCERC20(erc20Address);
            ibcERC20Contract.burn(_convertedAmount);
        }

        emit ICS20Transfer(packetData, erc20Address);
    }

    /// @inheritdoc IIBCApp
    function onRecvPacket(OnRecvPacketCallback calldata msg_) external onlyOwner nonReentrant returns (bytes memory) {
        // Since this function mostly returns acks, also when it fails, the ics26router (the caller) will log the ack
        if (keccak256(abi.encodePacked(msg_.packet.version)) != keccak256(abi.encodePacked(ICS20Lib.ICS20_VERSION))) {
            // TODO: Figure out if should actually error out, or if just error acking is enough
            return ICS20Lib.errorAck(abi.encodePacked("unexpected version: ", msg_.packet.version));
        }

        ICS20Lib.PacketDataJSON memory packetData = ICS20Lib.unmarshalJSON(msg_.packet.data);
        (address erc20Address, bool originatorChainIsSource) = getReceiveERC20AddressAndSource(msg_.packet, packetData);

        if (packetData.amount == 0) {
            return ICS20Lib.errorAck("invalid amount: 0");
        }

        (address receiver, bool receiverConvertSuccess) = ICS20Lib.hexStringToAddress(packetData.receiver);
        if (!receiverConvertSuccess) {
            return ICS20Lib.errorAck(abi.encodePacked("invalid receiver: ", packetData.receiver));
        }

        // Use SdkCoin.convertAmountSdkCoinToERC20 to consider the proper decimals conversions.
        uint256 _convertedAmount =
            SdkCoin.convertAmountSdkCoinToERC20(erc20Address, SafeCast.toUint64(packetData.amount));

        // TODO: Implement escrow balance tracking (#6)
        if (originatorChainIsSource) {
            // sender is source, so we mint vouchers
            // NOTE: getReceiveTokenContractAndSource has already created a contract with 6 decimals if it didn't exist
            IBCERC20(erc20Address).mint(_convertedAmount);
        }

        // transfer the tokens to the receiver
        IERC20(erc20Address).safeTransfer(receiver, _convertedAmount);

        // Note the event don't take into account the conversion
        emit ICS20ReceiveTransfer(packetData, erc20Address);

        return ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON;
    }

    /// @inheritdoc IIBCApp
    function onAcknowledgementPacket(OnAcknowledgementPacketCallback calldata msg_) external onlyOwner nonReentrant {
        ICS20Lib.PacketDataJSON memory packetData = ICS20Lib.unmarshalJSON(msg_.packet.data);

        if (keccak256(msg_.acknowledgement) != ICS20Lib.KECCAK256_SUCCESSFUL_ACKNOWLEDGEMENT_JSON) {
            (address erc20Address,) = getSendERC20AddressAndSource(msg_.packet, packetData);
            _refundTokens(packetData, erc20Address);
        }

        // Nothing needed to be done if the acknowledgement was successful, tokens are already in escrow or burnt
        emit ICS20Acknowledgement(packetData, msg_.acknowledgement);
    }

    /// @inheritdoc IIBCApp
    function onTimeoutPacket(OnTimeoutPacketCallback calldata msg_) external onlyOwner nonReentrant {
        ICS20Lib.PacketDataJSON memory packetData = ICS20Lib.unmarshalJSON(msg_.packet.data);
        (address erc20Address,) = getSendERC20AddressAndSource(msg_.packet, packetData);
        _refundTokens(packetData, erc20Address);

        emit ICS20Timeout(packetData);
    }

    /// @notice Refund the tokens to the sender
    /// @param packetData The packet data
    /// @param erc20Address The address of the ERC20 contract
    function _refundTokens(ICS20Lib.PacketDataJSON memory packetData, address erc20Address) private {
        address refundee = ICS20Lib.mustHexStringToAddress(packetData.sender);
        // Use SdkCoin.convertAmountSdkCoinToERC20 to consider the proper decimals conversions.
        (uint256 _convertedAmount) =
            SdkCoin.convertAmountSdkCoinToERC20(erc20Address, SafeCast.toUint64(packetData.amount));
        IERC20(erc20Address).safeTransfer(refundee, _convertedAmount);
    }

    /// @notice Transfer tokens from sender to receiver
    /// @param sender The sender of the tokens
    /// @param receiver The receiver of the tokens
    /// @param tokenContract The address of the token contract
    /// @param amount The amount of tokens to transfer
    function _transferFrom(address sender, address receiver, address tokenContract, uint256 amount) private {
        // we snapshot current balance of this token
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

    /// @notice For a send packet, get the ERC20 address for the token and whether the originator chain is the source
    /// @param packet The ICS26 packet
    /// @param packetData The unmarshalled packet data
    /// @return The ERC20 address for the token in the packetData
    /// @return Whether the originator (i.e. us) chain of the packet is the source of the token
    function getSendERC20AddressAndSource(
        IICS26RouterMsgs.Packet calldata packet,
        ICS20Lib.PacketDataJSON memory packetData
    )
        private
        view
        returns (address, bool)
    {
        bytes memory denomBz = bytes(packetData.denom);
        bytes memory sourceDenomPrefix = ICS20Lib.getDenomPrefix(packet.sourcePort, packet.sourceChannel);
        bool originatorChainIsSource = !ICS20Lib.hasPrefix(denomBz, sourceDenomPrefix);

        address erc20Address;
        if (originatorChainIsSource) {
            // we are the source of this token, so the denom should be the contract address
            erc20Address = ICS20Lib.mustHexStringToAddress(packetData.denom);
        } else {
            // receiving chain is source of the token, so we've received and mapped this token before
            string memory ibcDenom = ICS20Lib.toIBCDenom(packetData.denom);
            erc20Address = address(_foreignDenomContracts[ibcDenom]);
            if (erc20Address == address(0)) {
                revert ICS20DenomNotFound(packetData.denom);
            }
        }
        return (erc20Address, originatorChainIsSource);
    }

    /// @notice For a receive packet, get the ERC20 address for the token and whether the originator chain is the source
    /// @param packet The ICS26 packet
    /// @param packetData The unmarshalled packet data
    /// @return The ERC20 address for the token in the packetData
    /// @return Whether the originator (i.e. the counterparty) chain of the packet is the source of the token
    function getReceiveERC20AddressAndSource(
        IICS26RouterMsgs.Packet calldata packet,
        ICS20Lib.PacketDataJSON memory packetData
    )
        private
        returns (address, bool)
    {
        bytes memory denomBz = bytes(packetData.denom);
        bytes memory sourceDenomPrefix = ICS20Lib.getDenomPrefix(packet.sourcePort, packet.sourceChannel);
        bool originatorChainIsSource = !ICS20Lib.hasPrefix(denomBz, sourceDenomPrefix);

        address erc20Address;
        if (originatorChainIsSource) {
            // we are not the source of this token: we add a denom trace and find or create a new token contract
            string memory baseDenom = packetData.denom;
            bytes memory newDenomPrefix = ICS20Lib.getDenomPrefix(packet.destPort, packet.destChannel);
            string memory fullDenomPath = string(abi.encodePacked(newDenomPrefix, baseDenom));

            erc20Address = findOrCreateERC20Address(fullDenomPath, baseDenom);
        } else {
            // we are the source of this token: we remove the source prefix and expect the denom to be an erc20 address
            string memory erc20AddressStr =
                string(ICS20Lib.slice(denomBz, sourceDenomPrefix.length, denomBz.length - sourceDenomPrefix.length));
            erc20Address = ICS20Lib.mustHexStringToAddress(erc20AddressStr);
        }

        return (erc20Address, originatorChainIsSource);
    }

    /// @notice Finds a contract in the foreign mapping, or creates a new IBCERC20 contract
    /// @notice This function will never return address(0)
    /// @param fullDenomPath The full path denom to find or create the contract for
    /// @param base The base denom of the token, used when creating a new IBCERC20 contract
    /// @return The address of the erc20 contract
    function findOrCreateERC20Address(string memory fullDenomPath, string memory base) internal returns (address) {
        // check if denom already has a foreign registered contract
        string memory ibcDenom = ICS20Lib.toIBCDenom(fullDenomPath);
        address erc20Contract = address(_foreignDenomContracts[ibcDenom]);
        if (erc20Contract == address(0)) {
            // nothing exists, so we create new erc20 contract and register it in the mapping
            IBCERC20 ibcERC20 = new IBCERC20(IICS20Transfer(address(this)), ibcDenom, base, fullDenomPath);
            _foreignDenomContracts[ibcDenom] = ibcERC20;
            erc20Contract = address(ibcERC20);
        }

        return erc20Contract;
    }
}
