// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IICS26RouterMsgs } from "./msgs/IICS26RouterMsgs.sol";
import { IICS20TransferMsgs } from "./msgs/IICS20TransferMsgs.sol";

import { IICS20Errors } from "./errors/IICS20Errors.sol";
import { IEscrow } from "./interfaces/IEscrow.sol";
import { IIBCApp } from "./interfaces/IIBCApp.sol";
import { IERC20 } from "@openzeppelin-contracts/token/ERC20/IERC20.sol";
import { IICS20Transfer } from "./interfaces/IICS20Transfer.sol";
import { IICS26Router } from "./interfaces/IICS26Router.sol";
import { IIBCUUPSUpgradeable } from "./interfaces/IIBCUUPSUpgradeable.sol";
import { ISignatureTransfer } from "@uniswap/permit2/src/interfaces/ISignatureTransfer.sol";

import { ReentrancyGuardTransientUpgradeable } from
    "@openzeppelin-upgradeable/utils/ReentrancyGuardTransientUpgradeable.sol";
import { SafeERC20 } from "@openzeppelin-contracts/token/ERC20/utils/SafeERC20.sol";
import { MulticallUpgradeable } from "@openzeppelin-upgradeable/utils/MulticallUpgradeable.sol";
import { ICS20Lib } from "./utils/ICS20Lib.sol";
import { ICS24Host } from "./utils/ICS24Host.sol";
import { IBCERC20 } from "./utils/IBCERC20.sol";
import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";
import { Bytes } from "@openzeppelin-contracts/utils/Bytes.sol";
import { UUPSUpgradeable } from "@openzeppelin-contracts/proxy/utils/UUPSUpgradeable.sol";
import { IBCPausableUpgradeable } from "./utils/IBCPausableUpgradeable.sol";
import { ERC1967Proxy } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Proxy.sol";

using SafeERC20 for IERC20;

/*
 * Things not handled yet:
 * - Separate escrow balance tracking
 * - Related to escrow ^: invariant checking (where to implement that?)
 */
contract ICS20Transfer is
    IICS20Errors,
    IICS20Transfer,
    IIBCApp,
    ReentrancyGuardTransientUpgradeable,
    MulticallUpgradeable,
    UUPSUpgradeable,
    IBCPausableUpgradeable
{
    /// @notice Storage of the ICS20Transfer contract
    /// @dev It's implemented on a custom ERC-7201 namespace to reduce the risk of storage collisions when using with
    /// upgradeable contracts.
    /// @param escrows The escrow contract per client.
    /// @param ibcERC20Contracts Mapping of non-native denoms to their respective IBCERC20 contracts
    /// @param ics26Router The ICS26Router contract address. Immutable.
    /// @param ibcERC20Logic The address of the IBCERC20 logic contract. Immutable.
    /// @param escrowLogic The address of the Escrow logic contract. Immutable.
    /// @param permit2 The permit2 contract. Immutable.
    /// @custom:storage-location erc7201:ibc.storage.ICS20Transfer
    struct ICS20TransferStorage {
        mapping(string clientId => IEscrow escrow) escrows;
        mapping(string => IBCERC20) ibcERC20Contracts;
        mapping(address => string) ibcERC20Denoms;
        IICS26Router ics26Router;
        address ibcERC20Logic;
        address escrowLogic;
        ISignatureTransfer permit2;
    }

    /// @notice ERC-7201 slot for the ICS20Transfer storage
    /// @dev keccak256(abi.encode(uint256(keccak256("ibc.storage.ICS20Transfer")) - 1)) & ~bytes32(uint256(0xff))
    bytes32 private constant ICS20TRANSFER_STORAGE_SLOT =
        0x823f7a8ea9ae6df0eb03ec5e1682d7a2839417ad8a91774118e6acf2e8d2f800;

    /// @dev This contract is meant to be deployed by a proxy, so the constructor is not used
    constructor() {
        _disableInitializers();
    }

    /// @notice Initializes the contract instead of a constructor
    /// @dev Meant to be called only once from the proxy
    /// @param ics26Router The ICS26Router contract address
    /// @param escrowLogic The address of the Escrow logic contract
    /// @param pauser The address that can pause and unpause the contract
    /// @inheritdoc IICS20Transfer
    function initialize(
        address ics26Router,
        address escrowLogic,
        address ibcERC20Logic,
        address pauser,
        address permit2
    )
        public
        initializer
    {
        __ReentrancyGuardTransient_init();
        __Multicall_init();
        __IBCPausable_init(pauser);

        ICS20TransferStorage storage $ = _getICS20TransferStorage();

        $.ics26Router = IICS26Router(ics26Router);
        $.ibcERC20Logic = ibcERC20Logic;
        $.escrowLogic = escrowLogic;
        $.permit2 = ISignatureTransfer(permit2);
    }

    /// @inheritdoc IICS20Transfer
    function getEscrow(string calldata clientId) external view returns (address) {
        return address(_getICS20TransferStorage().escrows[clientId]);
    }

    /// @inheritdoc IICS20Transfer
    function ibcERC20Contract(string calldata denom) external view returns (address) {
        address contractAddress = address(_getICS20TransferStorage().ibcERC20Contracts[denom]);
        require(contractAddress != address(0), ICS20DenomNotFound(denom));
        return contractAddress;
    }

    /// @inheritdoc IICS20Transfer
    function sendTransfer(IICS20TransferMsgs.SendTransferMsg calldata msg_)
        external
        whenNotPaused
        nonReentrant
        returns (uint32)
    {
        require(msg_.amount > 0, IICS20Errors.ICS20InvalidAmount(0));
        // transfer the tokens to us (requires the allowance to be set)
        IEscrow escrow = _getOrCreateEscrow(msg_.sourceClient);
        _transferFrom(_msgSender(), address(escrow), msg_.denom, msg_.amount);

        return sendTransferFromEscrow(msg_);
    }

    /// @inheritdoc IICS20Transfer
    function permitSendTransfer(
        IICS20TransferMsgs.SendTransferMsg calldata msg_,
        ISignatureTransfer.PermitTransferFrom calldata permit,
        bytes calldata signature
    )
        external
        whenNotPaused
        nonReentrant
        returns (uint32 sequence)
    {
        require(msg_.amount > 0, IICS20Errors.ICS20InvalidAmount(0));
        require(
            permit.permitted.token == msg_.denom,
            IICS20Errors.ICS20Permit2TokenMismatch(permit.permitted.token, msg_.denom)
        );
        // transfer the tokens to us with permit
        IEscrow escrow = _getOrCreateEscrow(msg_.sourceClient);
        _getPermit2().permitTransferFrom(
            permit,
            ISignatureTransfer.SignatureTransferDetails({ to: address(escrow), requestedAmount: msg_.amount }),
            _msgSender(),
            signature
        );

        return sendTransferFromEscrow(msg_);
    }

    /// @notice Send a transfer after the funds have been transferred to escrow
    /// @param msg_ The message for sending a transfer
    /// @return sequence The sequence number of the packet created
    function sendTransferFromEscrow(IICS20TransferMsgs.SendTransferMsg calldata msg_) private returns (uint32) {
        string memory fullDenomPath = _getICS20TransferStorage().ibcERC20Denoms[msg_.denom];
        if (bytes(fullDenomPath).length == 0) {
            // if the denom is not mapped, it is a native token
            fullDenomPath = Strings.toHexString(msg_.denom);
        } else {
            // we have a mapped denom, so we need to check if the token is returning to source
            bytes memory prefix = ICS20Lib.getDenomPrefix(ICS20Lib.DEFAULT_PORT_ID, msg_.sourceClient);
            // if the denom is prefixed by the port and channel on which we are sending
            // the token, then we must be returning the token back to the chain they originated from
            bool returningToSource = ICS20Lib.hasPrefix(bytes(fullDenomPath), prefix);
            if (returningToSource) {
                // token is returning to source, it is an IBCERC20 and we must burn the token (not keep it in escrow)
                IBCERC20(msg_.denom).burn(msg_.amount);
            }
        }

        IICS20TransferMsgs.FungibleTokenPacketData memory packetData =
            IICS20TransferMsgs.FungibleTokenPacketData({
                denom: fullDenomPath,
                sender: Strings.toHexString(_msgSender()),
                receiver: msg_.receiver,
                amount: msg_.amount,
                memo: msg_.memo
            });

        return _getICS26Router().sendPacket(
            IICS26RouterMsgs.MsgSendPacket({
                sourceClient: msg_.sourceClient,
                timeoutTimestamp: msg_.timeoutTimestamp,
                payload: IICS26RouterMsgs.Payload({
                    sourcePort: ICS20Lib.DEFAULT_PORT_ID,
                    destPort: ICS20Lib.DEFAULT_PORT_ID,
                    version: ICS20Lib.ICS20_VERSION,
                    encoding: ICS20Lib.ICS20_ENCODING,
                    value: abi.encode(packetData)
                })
            })
        );
    }

    /// @inheritdoc IIBCApp
    function onRecvPacket(OnRecvPacketCallback calldata msg_)
        external
        onlyRouter
        nonReentrant
        whenNotPaused
        returns (bytes memory)
    {
        // Since this function mostly returns acks, also when it fails, the ics26router (the caller) will log the ack
        require(
            keccak256(bytes(msg_.payload.version)) == keccak256(bytes(ICS20Lib.ICS20_VERSION)),
            ICS20UnexpectedVersion(ICS20Lib.ICS20_VERSION, msg_.payload.version)
        );
        require(
            keccak256(bytes(msg_.payload.sourcePort)) == keccak256(bytes(ICS20Lib.DEFAULT_PORT_ID)),
            ICS20InvalidPort(ICS20Lib.DEFAULT_PORT_ID, msg_.payload.sourcePort)
        );

        IICS20TransferMsgs.FungibleTokenPacketData memory packetData =
            abi.decode(msg_.payload.value, (IICS20TransferMsgs.FungibleTokenPacketData));
        require(packetData.amount > 0, ICS20InvalidAmount(0));

        (bool isAddress, address receiver) = Strings.tryParseAddress(packetData.receiver);
        require(isAddress, ICS20InvalidAddress(packetData.receiver));

        bytes memory denomBz = bytes(packetData.denom);
        bytes memory prefix = ICS20Lib.getDenomPrefix(msg_.payload.sourcePort, msg_.sourceClient);

        // This is the prefix that would have been prefixed to the denomination
        // on sender chain IF and only if the token originally came from the
        // receiving chain.
        //
        // NOTE: We use SourcePort and SourceChannel here, because the counterparty
        // chain would have prefixed with DestPort and DestChannel when originally
        // receiving this token.
        bool returningToOrigin = ICS20Lib.hasPrefix(denomBz, prefix);

        IEscrow escrow = _getOrCreateEscrow(msg_.destinationClient);
        address erc20Address;
        if (returningToOrigin) {
            // we are the origin source of this token: it is either an IBCERC20 or a "native" ERC20:
            // remove the first hop to unwind the trace
            bytes memory newDenom = Bytes.slice(denomBz, prefix.length);

            (, erc20Address) = Strings.tryParseAddress(string(newDenom));
            if (erc20Address == address(0)) {
                // we are the origin source and the token must be an IBCERC20 (since it is not a native token)
                erc20Address = address(_getICS20TransferStorage().ibcERC20Contracts[string(newDenom)]);
                require(erc20Address != address(0), ICS20DenomNotFound(string(newDenom)));
            }
        } else {
            // we are not origin source, i.e. sender chain is the origin source: add denom trace and mint vouchers
            bytes memory newDenomPrefix = ICS20Lib.getDenomPrefix(msg_.payload.destPort, msg_.destinationClient);
            bytes memory newDenom = abi.encodePacked(newDenomPrefix, denomBz);

            erc20Address = _findOrCreateERC20Address(newDenom, denomBz, address(escrow));
            IBCERC20(erc20Address).mint(packetData.amount);
        }

        // transfer the tokens to the receiver
        escrow.send(IERC20(erc20Address), receiver, packetData.amount);

        return ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON;
    }

    /// @inheritdoc IIBCApp
    function onAcknowledgementPacket(OnAcknowledgementPacketCallback calldata msg_)
        external
        onlyRouter
        nonReentrant
        whenNotPaused
    {
        if (keccak256(msg_.acknowledgement) == ICS24Host.KECCAK256_UNIVERSAL_ERROR_ACK) {
            IICS20TransferMsgs.FungibleTokenPacketData memory packetData =
                abi.decode(msg_.payload.value, (IICS20TransferMsgs.FungibleTokenPacketData));

            _refundTokens(msg_.payload.sourcePort, msg_.sourceClient, packetData);
        }
    }

    /// @inheritdoc IIBCApp
    function onTimeoutPacket(OnTimeoutPacketCallback calldata msg_) external onlyRouter nonReentrant whenNotPaused {
        IICS20TransferMsgs.FungibleTokenPacketData memory packetData =
            abi.decode(msg_.payload.value, (IICS20TransferMsgs.FungibleTokenPacketData));
        _refundTokens(msg_.payload.sourcePort, msg_.sourceClient, packetData);
    }

    /// @notice Refund the tokens to the sender
    /// @param sourcePort The source port of the packet
    /// @param sourceClient The source client of the packet
    /// @param packetData The packet data
    function _refundTokens(
        string calldata sourcePort,
        string calldata sourceClient,
        IICS20TransferMsgs.FungibleTokenPacketData memory packetData
    )
        private
    {
        address refundee = ICS20Lib.mustHexStringToAddress(packetData.sender);
        IEscrow escrow = _getOrCreateEscrow(sourceClient);

        (bool returningToSource, address erc20Address) =
            _getSendingERC20Address(sourcePort, sourceClient, packetData.denom);

        if (returningToSource) {
            // if the token was returning to source, it was burned on send, so we mint it back now
            IBCERC20(erc20Address).mint(packetData.amount);
        }

        escrow.send(IERC20(erc20Address), refundee, packetData.amount);
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
        require(
            actualEndingBalance > ourStartingBalance && actualEndingBalance == expectedEndingBalance,
            ICS20UnexpectedERC20Balance(expectedEndingBalance, actualEndingBalance)
        );
    }

    /// @notice Finds a contract in the foreign mapping, or creates a new IBCERC20 contract
    /// @notice This function will never return address(0)
    /// @param fullDenomPath The full path denom to find or create the contract for (which will be the name for the
    /// token)
    /// @param base The base denom to find or create the contract for (which will be the symbol for the token)
    /// @param escrow The escrow contract address to use for the IBCERC20 contract
    /// @return The address of the erc20 contract
    function _findOrCreateERC20Address(
        bytes memory fullDenomPath,
        bytes memory base,
        address escrow
    )
        private
        returns (address)
    {
        ICS20TransferStorage storage $ = _getICS20TransferStorage();

        // check if denom already has a foreign registered contract
        address erc20Contract = address($.ibcERC20Contracts[string(fullDenomPath)]);
        if (erc20Contract == address(0)) {
            // nothing exists, so we create new erc20 contract and register it in the mapping
            ERC1967Proxy ibcERC20Proxy = new ERC1967Proxy(
                $.ibcERC20Logic,
                abi.encodeWithSelector(
                    IBCERC20.initialize.selector,
                    address(this),
                    escrow,
                    address($.ics26Router),
                    string(base),
                    string(fullDenomPath)
                )
            );
            $.ibcERC20Contracts[string(fullDenomPath)] = IBCERC20(address(ibcERC20Proxy));
            $.ibcERC20Denoms[address(ibcERC20Proxy)] = string(fullDenomPath);
            erc20Contract = address(ibcERC20Proxy);

            emit IBCERC20ContractCreated(erc20Contract, string(fullDenomPath));
        }

        return erc20Contract;
    }

    /// @notice Returns the address of the sending ERC20 contract
    /// @param sourcePort The source port of the packet
    /// @param sourceClient The source client of the packet
    /// @param denom The full path denom of the token
    /// @return returningToSource Whether the token is returning to the source chain
    /// @return erc20Address The address of the sending ERC20 contract
    function _getSendingERC20Address(
        string memory sourcePort,
        string calldata sourceClient,
        string memory denom
    )
        private
        view
        returns (bool returningToSource, address erc20Address)
    {
        bytes memory denomBz = bytes(denom);

        bytes memory prefix = ICS20Lib.getDenomPrefix(sourcePort, sourceClient);

        // if the denom is prefixed by the port and channel on which we are sending
        // the token, then we must be returning the token back to the chain they originated from
        returningToSource = ICS20Lib.hasPrefix(denomBz, prefix);
        if (returningToSource) {
            // receiving chain is source of the token, so we've received and mapped this token before
            erc20Address = address(_getICS20TransferStorage().ibcERC20Contracts[denom]);
        } else {
            // the receiving chain is not the source of the token, so the token is either a native token
            // or we are a middle chain and the token was minted (and mapped) here.
            // NOTE: We check if the token is mapped _first_, to avoid a scenario where someone has a base denom
            // that is an address on their chain, and we would parse it as an address and fail to find the
            // mapped contract (or worse, find a contract that is not the correct one).
            address denomIDContract = address(_getICS20TransferStorage().ibcERC20Contracts[denom]);
            if (denomIDContract != address(0)) {
                erc20Address = denomIDContract;
            } else {
                // the token is not mapped, so the token must be native
                erc20Address = ICS20Lib.mustHexStringToAddress(denom);
            }
        }
        require(erc20Address != address(0), ICS20DenomNotFound(denom));

        return (returningToSource, erc20Address);
    }

    /// @notice Returns the storage of the ICS20Transfer contract
    function _getICS20TransferStorage() private pure returns (ICS20TransferStorage storage $) {
        // solhint-disable-next-line no-inline-assembly
        assembly {
            $.slot := ICS20TRANSFER_STORAGE_SLOT
        }
    }

    /// @inheritdoc UUPSUpgradeable
    function _authorizeUpgrade(address) internal view override {
        address ics26Router = address(_getICS26Router());
        require(IIBCUUPSUpgradeable(ics26Router).isAdmin(_msgSender()), ICS20Unauthorized(_msgSender()));
    }

    /// @inheritdoc IBCPausableUpgradeable
    function _authorizeSetPauser(address) internal view override {
        address ics26Router = address(_getICS26Router());
        require(IIBCUUPSUpgradeable(ics26Router).isAdmin(_msgSender()), ICS20Unauthorized(_msgSender()));
    }

    /// @notice Returns the escrow contract for a client
    /// @param clientId The client ID
    /// @return The escrow contract address
    function _getOrCreateEscrow(string memory clientId) private returns (IEscrow) {
        ICS20TransferStorage storage $ = _getICS20TransferStorage();

        IEscrow escrow = $.escrows[clientId];
        if (address(escrow) == address(0)) {
            escrow = IEscrow(
                address(
                    new ERC1967Proxy(
                        $.escrowLogic,
                        abi.encodeWithSelector(IEscrow.initialize.selector, address(this), address($.ics26Router))
                    )
                )
            );
            $.escrows[clientId] = escrow;
        }

        return escrow;
    }

    /// @notice Returns the ICS26Router contract
    /// @return The ICS26Router contract address
    function _getICS26Router() private view returns (IICS26Router) {
        return _getICS20TransferStorage().ics26Router;
    }

    /// @notice Returns the permit2 contract
    /// @return The permit2 contract address
    function _getPermit2() private view returns (ISignatureTransfer) {
        return _getICS20TransferStorage().permit2;
    }

    /// @notice Modifier to check if the caller is the ICS26Router contract
    modifier onlyRouter() {
        require(_msgSender() == address(_getICS26Router()), ICS20Unauthorized(_msgSender()));
        _;
    }
}
