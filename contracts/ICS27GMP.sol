// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IICS26RouterMsgs } from "./msgs/IICS26RouterMsgs.sol";
import { IICS27GMPMsgs } from "./msgs/IICS27GMPMsgs.sol";
import { IIBCAppCallbacks } from "./msgs/IIBCAppCallbacks.sol";

import { IICS26Router } from "./interfaces/IICS26Router.sol";
import { IIBCApp } from "./interfaces/IIBCApp.sol";
import { IICS27GMP } from "./interfaces/IICS27GMP.sol";

import { ReentrancyGuardTransientUpgradeable } from
    "@openzeppelin-upgradeable/utils/ReentrancyGuardTransientUpgradeable.sol";
import { MulticallUpgradeable } from "@openzeppelin-upgradeable/utils/MulticallUpgradeable.sol";
import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";
import { ICS27Lib } from "./utils/ICS27Lib.sol";

/// @title ICS27 General Message Passing
/// @notice This contract is the implementation of the ics27-2 IBC specification for general message passing.
contract ICS27GMP is IICS27GMP, IIBCApp, ReentrancyGuardTransientUpgradeable, MulticallUpgradeable {
    /// @notice Storage of the ICS27GMP contract
    /// @dev It's implemented on a custom ERC-7201 namespace to reduce the risk of storage collisions when using with
    /// upgradeable contracts.
    /// @param _ics26 The ICS26Router contract address. Immutable.
    /// @custom:storage-location erc7201:ibc.storage.ICS27GMP
    struct ICS27GMPStorage {
        IICS26Router _ics26;
    }

    /// @notice ERC-7201 slot for the ICS27GMP storage
    /// @dev keccak256(abi.encode(uint256(keccak256("ibc.storage.ICS27GMP")) - 1)) & ~bytes32(uint256(0xff))
    bytes32 private constant ICS27GMP_STORAGE_SLOT =
        0xe73deb02cd654f25b90ec94b434589ea350a706e2446d278b41c3a86dc8f4500;

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
    function onRecvPacket(IIBCAppCallbacks.OnRecvPacketCallback calldata msg_) external nonReentrant returns (bytes memory) {
        revert("TODO: Not implemented");
    }

    /// @inheritdoc IIBCApp
    function onAcknowledgementPacket(IIBCAppCallbacks.OnAcknowledgementPacketCallback calldata msg_) nonReentrant external {
        revert("TODO: Not implemented");
    }

    /// @inheritdoc IIBCApp
    function onTimeoutPacket(IIBCAppCallbacks.OnTimeoutPacketCallback calldata msg_) nonReentrant external {
        revert("TODO: Not implemented");
    }

    /// @notice Returns the storage of the ICS27GMP contract
    function _getICS27GMPStorage() private pure returns (ICS27GMPStorage storage $) {
        // solhint-disable-next-line no-inline-assembly
        assembly {
            $.slot := ICS27GMP_STORAGE_SLOT
        }
    }
}
