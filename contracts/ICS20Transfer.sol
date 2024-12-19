// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IEscrow } from "./interfaces/IEscrow.sol";
import { IIBCApp } from "./interfaces/IIBCApp.sol";
import { IICS20Errors } from "./errors/IICS20Errors.sol";
import { ICS20Lib } from "./utils/ICS20Lib.sol";
import { IERC20 } from "@openzeppelin/token/ERC20/IERC20.sol";
import { SafeERC20 } from "@openzeppelin/token/ERC20/utils/SafeERC20.sol";
import { Ownable } from "@openzeppelin/access/Ownable.sol";
import { ReentrancyGuardTransient } from "@openzeppelin/utils/ReentrancyGuardTransient.sol";
import { Multicall } from "@openzeppelin/utils/Multicall.sol";
import { IICS20Transfer } from "./interfaces/IICS20Transfer.sol";
import { IICS26Router } from "./interfaces/IICS26Router.sol";
import { IICS26RouterMsgs } from "./msgs/IICS26RouterMsgs.sol";
import { IBCERC20 } from "./utils/IBCERC20.sol";
import { Escrow } from "./utils/Escrow.sol";
import { Pausable } from "@openzeppelin/utils/Pausable.sol";

using SafeERC20 for IERC20;

/**
 * @title ICS20Transfer
 * @notice Implements the ICS20 token transfer logic with pausable functionality.
 */
contract ICS20Transfer is IIBCApp, IICS20Transfer, IICS20Errors, Ownable, ReentrancyGuardTransient, Multicall, Pausable {
    /// @notice The escrow contract address
    IEscrow private ESCROW;

    /// @notice Mapping of non-native denoms to their respective IBCERC20 contracts created here
    mapping(string denom => IBCERC20 ibcERC20Contract) private _ibcDenomContracts;

    address private immutable SAFE_ADDRESS;

    /// @notice Constructor to initialize the contract with the Safe address
    /// @param _safeAddress The address of the Safe multisig
    constructor(address _safeAddress) Ownable(address(0xdead)) {
        SAFE_ADDRESS = _safeAddress; // This should be hardcoded after deployment in a real setup.
    }

    /// @notice Initialize the contract
    /// @param _safeAddress The Safe multisig address
    function initialize(address _safeAddress) external {
        require(owner() == address(0), "Already initialized");
        require(_safeAddress == SAFE_ADDRESS, "Only Safe can initialize");

        _transferOwnership(SAFE_ADDRESS); // Transfer ownership to Safe
        ESCROW = new Escrow(address(this));
    }

    /// @inheritdoc IICS20Transfer
    function escrow() external view override returns (address) {
        return address(ESCROW);
    }

    /// @inheritdoc IICS20Transfer
    function ibcERC20Contract(string calldata denom) external view returns (address) {
        address contractAddress = address(_ibcDenomContracts[denom]);
        require(contractAddress != address(0), ICS20DenomNotFound(denom));
        return contractAddress;
    }

    /// @inheritdoc IICS20Transfer
    function newMsgSendPacketV1(
        address sender,
        SendTransferMsg calldata msg_
    )
        external
        view
        override
        whenNotPaused
        returns (IICS26RouterMsgs.MsgSendPacket memory)
    {
        return ICS20Lib.newMsgSendPacketV1(sender, msg_);
    }

    /// @inheritdoc IICS20Transfer
    function sendTransfer(SendTransferMsg calldata msg_) external override whenNotPaused returns (uint32) {
        return IICS26Router(owner()).sendPacket(ICS20Lib.newMsgSendPacketV1(_msgSender(), msg_));
    }

    /// @inheritdoc IIBCApp
    function onSendPacket(OnSendPacketCallback calldata msg_) external onlyOwner nonReentrant whenNotPaused {
        require(
            keccak256(bytes(msg_.payload.version)) == keccak256(bytes(ICS20Lib.ICS20_VERSION)),
            ICS20UnexpectedVersion(ICS20Lib.ICS20_VERSION, msg_.payload.version)
        );

        ICS20Lib.FungibleTokenPacketData memory packetData =
            abi.decode(msg_.payload.value, (ICS20Lib.FungibleTokenPacketData));

        require(packetData.amount > 0, ICS20InvalidAmount(packetData.amount));

        address sender = ICS20Lib.mustHexStringToAddress(packetData.sender);

        require(msg_.sender == sender || msg_.sender == address(this), ICS20UnauthorizedPacketSender(msg_.sender));

        (address erc20Address, bool originatorChainIsSource) =
            getSendERC20AddressAndSource(msg_.payload.sourcePort, msg_.sourceChannel, packetData);

        _transferFrom(sender, address(ESCROW), erc20Address, packetData.amount);

        if (!originatorChainIsSource) {
            IBCERC20 ibcERC20 = IBCERC20(erc20Address);
            ibcERC20.burn(packetData.amount);
        }
    }

    /// @inheritdoc IIBCApp
    function onRecvPacket(OnRecvPacketCallback calldata msg_) external onlyOwner nonReentrant whenNotPaused returns (bytes memory) {
        if (keccak256(bytes(msg_.payload.version)) != keccak256(bytes(ICS20Lib.ICS20_VERSION))) {
            return ICS20Lib.errorAck(abi.encodePacked("unexpected version: ", msg_.payload.version));
        }

        ICS20Lib.FungibleTokenPacketData memory packetData =
            abi.decode(msg_.payload.value, (ICS20Lib.FungibleTokenPacketData));

        (address erc20Address, bool originatorChainIsSource) = getReceiveERC20AddressAndSource(
            msg_.payload.sourcePort, msg_.sourceChannel, msg_.payload.destPort, msg_.destinationChannel, packetData
        );

        if (packetData.amount == 0) {
            return ICS20Lib.errorAck("invalid amount: 0");
        }

        (address receiver, bool receiverConvertSuccess) = ICS20Lib.hexStringToAddress(packetData.receiver);
        if (!receiverConvertSuccess) {
            return ICS20Lib.errorAck(abi.encodePacked("invalid receiver: ", packetData.receiver));
        }

        if (originatorChainIsSource) {
            IBCERC20(erc20Address).mint(packetData.amount);
        }

        ESCROW.send(IERC20(erc20Address), receiver, packetData.amount);

        return ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON;
    }

    /// @inheritdoc IIBCApp
    function onAcknowledgementPacket(OnAcknowledgementPacketCallback calldata msg_) external onlyOwner nonReentrant whenNotPaused {
        ICS20Lib.FungibleTokenPacketData memory packetData =
            abi.decode(msg_.payload.value, (ICS20Lib.FungibleTokenPacketData));

        if (keccak256(msg_.acknowledgement) != ICS20Lib.KECCAK256_SUCCESSFUL_ACKNOWLEDGEMENT_JSON) {
            (address erc20Address,) =
                getSendERC20AddressAndSource(msg_.payload.sourcePort, msg_.sourceChannel, packetData);
            _refundTokens(packetData, erc20Address);
        }
    }

    /// @inheritdoc IIBCApp
    function onTimeoutPacket(OnTimeoutPacketCallback calldata msg_) external onlyOwner nonReentrant whenNotPaused {
        ICS20Lib.FungibleTokenPacketData memory packetData =
            abi.decode(msg_.payload.value, (ICS20Lib.FungibleTokenPacketData));
        (address erc20Address,) = getSendERC20AddressAndSource(msg_.payload.sourcePort, msg_.sourceChannel, packetData);
        _refundTokens(packetData, erc20Address);
    }

    /// @notice Pause the contract (only callable by owner)
    function pause() external onlyOwner {
        _pause();
    }

    /// @notice Unpause the contract (only callable by owner)
    function unpause() external onlyOwner {
        _unpause();
    }

    /// @notice Refund the tokens to the sender
    function _refundTokens(ICS20Lib.FungibleTokenPacketData memory packetData, address erc20Address) private {
        address refundee = ICS20Lib.mustHexStringToAddress(packetData.sender);
        ESCROW.send(IERC20(erc20Address), refundee, packetData.amount);
    }

    /// @notice Transfer tokens from sender to receiver
    function _transferFrom(address sender, address receiver, address tokenContract, uint256 amount) private {
        uint256 ourStartingBalance = IERC20(tokenContract).balanceOf(receiver);

        IERC20(tokenContract).safeTransferFrom(sender, receiver, amount);

        uint256 actualEndingBalance = IERC20(tokenContract).balanceOf(receiver);

        uint256 expectedEndingBalance = ourStartingBalance + amount;
        require(
            actualEndingBalance > ourStartingBalance && actualEndingBalance == expectedEndingBalance,
            ICS20UnexpectedERC20Balance(expectedEndingBalance, actualEndingBalance)
        );
    }

    /// @notice For a send packet, get the ERC20 address for the token and whether the originator chain is the source
    /// @param sourcePort The source port of the packet
    /// @param sourceChannel The source channel of the packet
    /// @param packetData The unmarshalled packet data
    /// @return The ERC20 address for the token in the packetData
    /// @return Whether the originator (i.e. us) chain of the packet is the source of the token
    function getSendERC20AddressAndSource(
        string calldata sourcePort,
        string calldata sourceChannel,
        ICS20Lib.FungibleTokenPacketData memory packetData
    )
        private
        view
        returns (address, bool)
    {
        bytes memory denomBz = bytes(packetData.denom);
        bytes memory sourceDenomPrefix = ICS20Lib.getDenomPrefix(sourcePort, sourceChannel);
        bool originatorChainIsSource = !ICS20Lib.hasPrefix(denomBz, sourceDenomPrefix);

        address erc20Address;
        if (originatorChainIsSource) {
            // we are the source of this token, so the denom should be the contract address
            erc20Address = ICS20Lib.mustHexStringToAddress(packetData.denom);
        } else {
            // receiving chain is source of the token, so we've received and mapped this token before
            string memory ibcDenom = ICS20Lib.toIBCDenom(packetData.denom);
            erc20Address = address(_ibcDenomContracts[ibcDenom]);
            require(erc20Address != address(0), ICS20DenomNotFound(packetData.denom));
        }
        return (erc20Address, originatorChainIsSource);
    }

    /// @notice For a receive packet, get the ERC20 address for the token and whether the originator chain is the source
    /// @param sourcePort The source port of the packet
    /// @param sourceChannel The source channel of the packet
    /// @param destPort The destination port of the packet
    /// @param destChannel The destination channel of the packet
    /// @param packetData The unmarshalled packet data
    /// @return The ERC20 address for the token in the packetData
    /// @return Whether the originator (i.e. the counterparty) chain of the packet is the source of the token
    function getReceiveERC20AddressAndSource(
        string calldata sourcePort,
        string calldata sourceChannel,
        string calldata destPort,
        string calldata destChannel,
        ICS20Lib.FungibleTokenPacketData memory packetData
    )
        private
        returns (address, bool)
    {
        bytes memory denomBz = bytes(packetData.denom);
        bytes memory sourceDenomPrefix = ICS20Lib.getDenomPrefix(sourcePort, sourceChannel);
        bool originatorChainIsSource = !ICS20Lib.hasPrefix(denomBz, sourceDenomPrefix);

        address erc20Address;
        if (originatorChainIsSource) {
            // we are not the source of this token: we add a denom trace and find or create a new token contract
            string memory baseDenom = packetData.denom;
            bytes memory newDenomPrefix = ICS20Lib.getDenomPrefix(destPort, destChannel);
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
        address erc20Contract = address(_ibcDenomContracts[ibcDenom]);
        if (erc20Contract == address(0)) {
            // nothing exists, so we create new erc20 contract and register it in the mapping
            IBCERC20 ibcERC20 = new IBCERC20(this, ESCROW, ibcDenom, base, fullDenomPath);
            _ibcDenomContracts[ibcDenom] = ibcERC20;
            erc20Contract = address(ibcERC20);
        }

        return erc20Contract;
    }
}
