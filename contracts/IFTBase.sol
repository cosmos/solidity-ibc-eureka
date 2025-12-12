// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IIFTMsgs } from "./msgs/IIFTMsgs.sol";
import { IIBCAppCallbacks } from "./msgs/IIBCAppCallbacks.sol";
import { IICS27GMPMsgs } from "./msgs/IICS27GMPMsgs.sol";

import { IIFT } from "./interfaces/IIFT.sol";
import { IICS27GMP } from "./interfaces/IICS27GMP.sol";
import { IIFTSendCallConstructor } from "./interfaces/IIFTSendCallConstructor.sol";
import { IIBCSenderCallbacks } from "./interfaces/IIBCSenderCallbacks.sol";
import { IIFTErrors } from "./errors/IIFTErrors.sol";

import { ERC20 } from "@openzeppelin-contracts/token/ERC20/ERC20.sol";
import { AccessManaged } from "@openzeppelin-contracts/access/manager/AccessManaged.sol";
import { IBCCallbackReceiver } from "./utils/IBCCallbackReceiver.sol";

/// @title IFT Base Contract
/// @notice Abstract base contract for Interchain Fungible Tokens
/// @dev Extend this contract and implement the ERC20 constructor to create an IFT token
abstract contract IFTBase is IIFTErrors, IIFT, ERC20, AccessManaged, IBCCallbackReceiver {
    /// @notice Storage for IFT-specific state
    /// @param _ics27Gmp The ICS27-GMP contract for sending cross-chain messages
    /// @param _iftBridges Mapping of client IDs to their bridge configurations
    /// @param _pendingTransfers Mapping of (clientId, sequence) to pending transfer info
    struct IFTStorage {
        IICS27GMP _ics27Gmp;
        mapping(string clientId => IIFTMsgs.IFTBridge bridge) _iftBridges;
        mapping(string clientId => mapping(uint64 seq => IIFTMsgs.PendingTransfer info)) _pendingTransfers;
    }

    /// @notice ERC-7201 slot for the IFT storage
    /// @dev keccak256(abi.encode(uint256(keccak256("ibc.storage.IFT")) - 1)) & ~bytes32(uint256(0xff))
    bytes32 private constant IFT_STORAGE_SLOT = 0x35d0029e62ce5824ad5e38215107659b8aa50b0046e8bc44a0f4a32b87d61a00;

    /// @notice Default timeout duration for IFT transfers (15 minutes)
    uint64 private constant DEFAULT_TIMEOUT_DURATION = 15 minutes;

    /// @notice Initializes the IFT base contract
    /// @param ics27Gmp_ The ICS27-GMP contract address
    /// @param authority_ The AccessManager contract address for access control
    constructor(IICS27GMP ics27Gmp_, address authority_) AccessManaged(authority_) {
        IFTStorage storage $ = _getIFTStorage();
        $._ics27Gmp = ics27Gmp_;
    }

    /// @inheritdoc IIFT
    function registerIFTBridge(
        string calldata clientId,
        string calldata counterpartyIFTAddress,
        address iftSendCallConstructor
    )
        external
        restricted
    {
        require(bytes(clientId).length > 0, IFTEmptyClientId());
        require(bytes(counterpartyIFTAddress).length > 0, IFTEmptyCounterpartyAddress());
        require(iftSendCallConstructor != address(0), IFTZeroAddressConstructor());

        IFTStorage storage $ = _getIFTStorage();
        $._iftBridges[clientId] = IIFTMsgs.IFTBridge({
            clientId: clientId,
            counterpartyIFTAddress: counterpartyIFTAddress,
            iftSendCallConstructor: IIFTSendCallConstructor(iftSendCallConstructor)
        });

        emit IFTBridgeRegistered(clientId, counterpartyIFTAddress, iftSendCallConstructor);
    }

    /// @inheritdoc IIFT
    function iftTransfer(
        string calldata clientId,
        string calldata receiver,
        uint256 amount,
        uint64 timeoutTimestamp
    )
        external
    {
        _iftTransfer(_msgSender(), clientId, receiver, amount, timeoutTimestamp);
    }

    /// @inheritdoc IIFT
    function iftTransfer(string calldata clientId, string calldata receiver, uint256 amount) external {
        uint64 timeoutTimestamp = uint64(block.timestamp) + DEFAULT_TIMEOUT_DURATION;
        _iftTransfer(_msgSender(), clientId, receiver, amount, timeoutTimestamp);
    }

    /// @notice Internal implementation of iftTransfer
    /// @param sender The address initiating the transfer
    /// @param clientId The IBC client identifier
    /// @param receiver The receiver address on the counterparty chain
    /// @param amount The amount of tokens to transfer
    /// @param timeoutTimestamp The timeout timestamp for the IBC packet
    function _iftTransfer(
        address sender,
        string calldata clientId,
        string calldata receiver,
        uint256 amount,
        uint64 timeoutTimestamp
    )
        internal
    {
        require(bytes(clientId).length > 0, IFTEmptyClientId());
        require(bytes(receiver).length > 0, IFTEmptyReceiver());
        require(amount > 0, IFTZeroAmount());
        require(timeoutTimestamp > block.timestamp, IFTTimeoutInPast(timeoutTimestamp, uint64(block.timestamp)));

        _burn(sender, amount); // Implemented in the ERC20 base contract

        IFTStorage storage $ = _getIFTStorage();
        IIFTMsgs.IFTBridge memory bridge = $._iftBridges[clientId];
        require(keccak256(bytes(bridge.clientId)) == keccak256(bytes(clientId)), IFTBridgeNotFound(clientId));

        bytes memory payload = bridge.iftSendCallConstructor.constructMintCall(receiver, amount);
        uint64 seq = $._ics27Gmp
            .sendCall(
                IICS27GMPMsgs.SendCallMsg({
                    sourceClient: bridge.clientId,
                    receiver: bridge.counterpartyIFTAddress,
                    salt: "",
                    payload: payload,
                    timeoutTimestamp: timeoutTimestamp,
                    memo: ""
                })
            );

        $._pendingTransfers[bridge.clientId][seq] = IIFTMsgs.PendingTransfer({ sender: sender, amount: amount });

        emit IFTTransferInitiated(bridge.clientId, seq, sender, receiver, amount);
    }

    /// @inheritdoc IIFT
    function iftMint(address receiver, uint256 amount) external {
        IFTStorage storage $ = _getIFTStorage();

        IICS27GMPMsgs.AccountIdentifier memory accountId = $._ics27Gmp.getAccountIdentifier(_msgSender());
        IIFTMsgs.IFTBridge memory bridge = $._iftBridges[accountId.clientId];

        require(
            keccak256(bytes(bridge.clientId)) == keccak256(bytes(accountId.clientId)),
            IFTBridgeNotFound(accountId.clientId)
        );
        require(
            keccak256(bytes(bridge.counterpartyIFTAddress)) == keccak256(bytes(accountId.sender)),
            IFTUnauthorizedMint(bridge.counterpartyIFTAddress, accountId.sender)
        );
        require(accountId.salt.length == 0, IFTUnexpectedSalt(accountId.salt));

        _mint(receiver, amount); // Implemented in the ERC20 base contract

        emit IFTMintReceived(accountId.clientId, receiver, amount);
    }

    /// @inheritdoc IIFT
    function getIFTBridge(string calldata clientId) external view returns (IIFTMsgs.IFTBridge memory) {
        return _getIFTStorage()._iftBridges[clientId];
    }

    /// @inheritdoc IIFT
    function getPendingTransfer(
        string calldata clientId,
        uint64 sequence
    )
        external
        view
        returns (IIFTMsgs.PendingTransfer memory)
    {
        return _getIFTStorage()._pendingTransfers[clientId][sequence];
    }

    /// @inheritdoc IIFT
    function ics27() external view returns (IICS27GMP) {
        return _getIFTStorage()._ics27Gmp;
    }

    /// @inheritdoc IIBCSenderCallbacks
    function onAckPacket(
        bool success,
        IIBCAppCallbacks.OnAcknowledgementPacketCallback calldata msg_
    )
        external
        override
    {
        _onlyICS27GMP(); // Ensure only ICS27-GMP can call this function

        if (!success) {
            _refundPendingTransfer(msg_.sourceClient, msg_.sequence);
            return;
        }

        IFTStorage storage $ = _getIFTStorage();
        IIFTMsgs.PendingTransfer memory pending = $._pendingTransfers[msg_.sourceClient][msg_.sequence];

        require(pending.amount > 0, IFTPendingTransferNotFound(msg_.sourceClient, msg_.sequence));

        delete $._pendingTransfers[msg_.sourceClient][msg_.sequence];

        emit IFTTransferCompleted(msg_.sourceClient, msg_.sequence, pending.sender, pending.amount);
    }

    /// @inheritdoc IIBCSenderCallbacks
    function onTimeoutPacket(IIBCAppCallbacks.OnTimeoutPacketCallback calldata msg_) external override {
        _onlyICS27GMP(); // Ensure only ICS27-GMP can call this function

        _refundPendingTransfer(msg_.sourceClient, msg_.sequence);
    }

    /// @notice Refunds a pending transfer back to the sender
    /// @param clientId The IBC client identifier
    /// @param sequence The packet sequence number
    function _refundPendingTransfer(string memory clientId, uint64 sequence) internal {
        IFTStorage storage $ = _getIFTStorage();
        IIFTMsgs.PendingTransfer memory pending = $._pendingTransfers[clientId][sequence];

        require(pending.amount > 0, IFTPendingTransferNotFound(clientId, sequence));

        _mint(pending.sender, pending.amount); // Implemented in the ERC20 base contract
        delete $._pendingTransfers[clientId][sequence];

        emit IFTTransferRefunded(clientId, sequence, pending.sender, pending.amount);
    }

    /// @notice Ensures the caller is the ICS27-GMP contract
    function _onlyICS27GMP() internal view {
        IFTStorage storage $ = _getIFTStorage();
        require(_msgSender() == address($._ics27Gmp), IFTOnlyICS27GMP(_msgSender()));
    }

    /// @notice Returns the IFT storage
    /// @return $ The IFT storage struct
    function _getIFTStorage() internal pure returns (IFTStorage storage $) {
        // solhint-disable-next-line no-inline-assembly
        assembly {
            $.slot := IFT_STORAGE_SLOT
        }
    }
}
