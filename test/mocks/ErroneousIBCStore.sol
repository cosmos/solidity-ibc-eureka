// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

// solhint-disable no-empty-blocks

import { IIBCStore } from "../../src/interfaces/IIBCStore.sol";
import { IICS26RouterMsgs } from "../../src/msgs/IICS26RouterMsgs.sol";

/// @title Erroneous IBC Store
/// @dev This contract is used to override some functions of the IBC store contract using cheatcodes
contract ErroneousIBCStore is IIBCStore {
    error CallFailure(string reason);

    constructor() { }

    function getCommitment(bytes32) external pure returns (bytes32) {
        revert CallFailure("getCommitment");
    }

    function nextSequenceSend(string calldata) external pure returns (uint32) {
        revert CallFailure("nextSequenceSend");
    }

    function commitPacket(IICS26RouterMsgs.Packet memory) external pure {
        revert CallFailure("commitPacket");
    }

    function deletePacketCommitment(IICS26RouterMsgs.Packet memory) external pure returns (bytes32) {
        revert CallFailure("deletePacketCommitment");
    }

    function setPacketReceipt(IICS26RouterMsgs.Packet memory) external pure {
        revert CallFailure("setPacketReceipt");
    }

    function commitPacketAcknowledgement(IICS26RouterMsgs.Packet memory, bytes[] memory) external pure {
        revert CallFailure("commitPacketAcknowledgement");
    }
}
