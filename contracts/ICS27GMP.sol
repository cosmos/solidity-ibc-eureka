// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IICS26RouterMsgs } from "./msgs/IICS26RouterMsgs.sol";
import { IICS27GMPMsgs } from "./msgs/IICS27GMPMsgs.sol";
import { IIBCAppCallbacks } from "./msgs/IIBCAppCallbacks.sol";

import { IICS26Router } from "./interfaces/IICS26Router.sol";
import { IIBCApp } from "./interfaces/IIBCApp.sol";
import { IICS27GMP } from "./interfaces/IICS27GMP.sol";
import { IICS27Account } from "./interfaces/IICS27Account.sol";
import { IICS27Errors } from "./errors/IICS27Errors.sol";
import { IIBCUUPSUpgradeable } from "./interfaces/IIBCUUPSUpgradeable.sol";

import { ReentrancyGuardTransientUpgradeable } from
    "@openzeppelin-upgradeable/utils/ReentrancyGuardTransientUpgradeable.sol";
import { MulticallUpgradeable } from "@openzeppelin-upgradeable/utils/MulticallUpgradeable.sol";
import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";
import { Create2 } from "@openzeppelin-contracts/utils/Create2.sol";
import { UUPSUpgradeable } from "@openzeppelin-contracts/proxy/utils/UUPSUpgradeable.sol";
import { UpgradeableBeacon } from "@openzeppelin-contracts/proxy/beacon/UpgradeableBeacon.sol";
import { ICS27Lib } from "./utils/ICS27Lib.sol";

/// @title ICS27 General Message Passing
/// @notice This contract is the implementation of the ics27-2 IBC specification for general message passing.
contract ICS27GMP is IICS27Errors, IICS27GMP, IIBCApp, ReentrancyGuardTransientUpgradeable, MulticallUpgradeable, UUPSUpgradeable {
    /// @notice Storage of the ICS27GMP contract
    /// @dev It's implemented on a custom ERC-7201 namespace to reduce the risk of storage collisions when using with
    /// upgradeable contracts.
    /// @param _accounts The mapping of account identifiers to account contracts.
    /// @param _ics26 The ICS26Router contract address. Immutable.
    /// @param _accountBeacon The address of the ICS27Account beacon contract. Immutable.
    /// @custom:storage-location erc7201:ibc.storage.ICS27GMP
    struct ICS27GMPStorage {
        mapping(bytes32 accountIdHash => IICS27Account account) _accounts;
        IICS26Router _ics26;
        UpgradeableBeacon _accountBeacon;
    }

    /// @notice ERC-7201 slot for the ICS27GMP storage
    /// @dev keccak256(abi.encode(uint256(keccak256("ibc.storage.ICS27GMP")) - 1)) & ~bytes32(uint256(0xff))
    bytes32 private constant ICS27GMP_STORAGE_SLOT = 0xe73deb02cd654f25b90ec94b434589ea350a706e2446d278b41c3a86dc8f4500;

    /// @dev This contract is meant to be deployed by a proxy, so the constructor is not used
    constructor() {
        _disableInitializers();
    }

    /// @inheritdoc IICS27GMP
    function initialize(address ics26_, address accountLogic) external initializer {
        __ReentrancyGuardTransient_init();
        __Multicall_init();

        ICS27GMPStorage storage $ = _getICS27GMPStorage();
        $._ics26 = IICS26Router(ics26_);
        $._accountBeacon = new UpgradeableBeacon(accountLogic, address(this));
    }

    /// @inheritdoc IICS27GMP
    function ics26() external view returns (address) {
        return address(_getICS27GMPStorage()._ics26);
    }

    /// @inheritdoc IICS27GMP
    function getAccountBeacon() external view returns (address) {
        return address(_getICS27GMPStorage()._accountBeacon);
    }

    /// @inheritdoc IICS27GMP
    function sendCall(IICS27GMPMsgs.SendCallMsg calldata msg_) external nonReentrant returns (uint64) {
        IICS27GMPMsgs.GMPPacketData memory packetData = IICS27GMPMsgs.GMPPacketData({
            sender: Strings.toHexString(_msgSender()),
            receiver: msg_.receiver,
            salt: msg_.salt,
            payload: msg_.payload,
            memo: msg_.memo
        });

        return _getICS27GMPStorage()._ics26.sendPacket(
            IICS26RouterMsgs.MsgSendPacket({
                sourceClient: msg_.sourceClient,
                timeoutTimestamp: msg_.timeoutTimestamp,
                payload: IICS26RouterMsgs.Payload({
                    sourcePort: ICS27Lib.DEFAULT_PORT_ID,
                    destPort: ICS27Lib.DEFAULT_PORT_ID,
                    version: ICS27Lib.ICS27_VERSION,
                    encoding: ICS27Lib.ICS27_ENCODING,
                    value: abi.encode(packetData)
                })
            })
        );
    }

    /// @inheritdoc IIBCApp
    function onRecvPacket(IIBCAppCallbacks.OnRecvPacketCallback calldata msg_)
        external
        nonReentrant
        onlyRouter
        returns (bytes memory)
    {
        require(
            keccak256(bytes(msg_.payload.version)) == ICS27Lib.KECCAK256_ICS27_VERSION,
            ICS27UnexpectedVersion(ICS27Lib.ICS27_VERSION, msg_.payload.version)
        );
        require(
            keccak256(bytes(msg_.payload.sourcePort)) == ICS27Lib.KECCAK256_DEFAULT_PORT_ID,
            ICS27InvalidPort(ICS27Lib.DEFAULT_PORT_ID, msg_.payload.sourcePort)
        );
        require(
            keccak256(bytes(msg_.payload.encoding)) == ICS27Lib.KECCAK256_ICS27_ENCODING,
            ICS27UnexpectedEncoding(ICS27Lib.ICS27_ENCODING, msg_.payload.encoding)
        );
        require(
            keccak256(bytes(msg_.payload.destPort)) == ICS27Lib.KECCAK256_DEFAULT_PORT_ID,
            ICS27InvalidPort(ICS27Lib.DEFAULT_PORT_ID, msg_.payload.destPort)
        );

        IICS27GMPMsgs.GMPPacketData memory packetData = abi.decode(msg_.payload.value, (IICS27GMPMsgs.GMPPacketData));

        IICS27GMPMsgs.AccountIdentifier memory accountId = IICS27GMPMsgs.AccountIdentifier({
            clientId: msg_.destinationClient,
            sender: packetData.sender,
            salt: packetData.salt
        });
        IICS27Account account = _getOrCreateAccount(accountId);

        (bool success, address receiver) = Strings.tryParseAddress(packetData.receiver);
        require(success, ICS27InvalidReceiver(packetData.receiver));

        return account.functionCall(receiver, packetData.payload);
    }

    /// @inheritdoc IIBCApp
    function onAcknowledgementPacket(IIBCAppCallbacks.OnAcknowledgementPacketCallback calldata msg_)
        external
        nonReentrant
        onlyRouter
    { }
    // solhint-disable-previous-line no-empty-blocks

    /// @inheritdoc IIBCApp
    function onTimeoutPacket(IIBCAppCallbacks.OnTimeoutPacketCallback calldata msg_) external nonReentrant onlyRouter { }
    // solhint-disable-previous-line no-empty-blocks

    /// @notice Creates or retrieves an account contract for the given account identifier
    /// @param accountId The account identifier
    /// @return account The account contract address
    function _getOrCreateAccount(IICS27GMPMsgs.AccountIdentifier memory accountId) private returns (IICS27Account) {
        ICS27GMPStorage storage $ = _getICS27GMPStorage();

        bytes32 accountIdHash = keccak256(abi.encode(accountId));
        IICS27Account account = $._accounts[accountIdHash];
        if (address(account) != address(0)) {
            return account;
        }

        bytes memory bytecode = ICS27Lib.getBeaconProxyBytecode(address($._accountBeacon), address(this));
        address accountAddress = Create2.deploy(0, accountIdHash, bytecode);

        $._accounts[accountIdHash] = IICS27Account(accountAddress);
        return IICS27Account(accountAddress);
    }

    /// @inheritdoc IICS27GMP
    function getOrComputeAccountAddress(IICS27GMPMsgs.AccountIdentifier calldata accountId)
        external
        view
        returns (address)
    {
        ICS27GMPStorage storage $ = _getICS27GMPStorage();

        bytes32 accountIdHash = keccak256(abi.encode(accountId));
        address account = address($._accounts[accountIdHash]);
        if (account != address(0)) {
            return account;
        }

        bytes32 codeHash = ICS27Lib.getBeaconProxyCodeHash(address($._accountBeacon), address(this));
        return Create2.computeAddress(accountIdHash, codeHash);
    }

    /// @notice Returns the storage of the ICS27GMP contract
    function _getICS27GMPStorage() private pure returns (ICS27GMPStorage storage $) {
        // solhint-disable-next-line no-inline-assembly
        assembly {
            $.slot := ICS27GMP_STORAGE_SLOT
        }
    }

    /// @inheritdoc UUPSUpgradeable
    function _authorizeUpgrade(address) internal view override onlyAdmin { }
    // solhint-disable-previous-line no-empty-blocks

    /// @notice Modifier to check if the caller is the ICS26Router contract
    modifier onlyRouter() {
        address router = address(_getICS27GMPStorage()._ics26);
        require(_msgSender() == router, ICS27Unauthorized(router, _msgSender()));
        _;
    }

    /// @notice Modifier to check if the caller is an admin via the ICS26Router contract
    modifier onlyAdmin() {
        address router = address(_getICS27GMPStorage()._ics26);
        require(IIBCUUPSUpgradeable(router).isAdmin(_msgSender()), ICS27Unauthorized(router, _msgSender()));
        _;
    }
}
