// SPDX-License-Identifier: MIT
pragma solidity >=0.8.25;

import {IICS26RouterMsgs} from "../msgs/IICS26RouterMsgs.sol";
import {ILightClientMsgs} from "../msgs/ILightClientMsgs.sol";

interface IICS26RouterErrors {
    /// @param portId port identifier
    error IBCPortAlreadyExists(string portId);

    /// @param portId port identifier
    error IBCInvalidPortIdentifier(string portId);

    /// @param timeoutTimestamp timeout timestamp in seconds
    error IBCInvalidTimeoutTimestamp(uint256 timeoutTimestamp, uint256 comparedTimestamp);

    /// @param expected expected counterparty identifier
    /// @param actual actual counterparty identifier
    error IBCInvalidCounterparty(string expected, string actual);

    error IBCAsyncAcknowledgementNotSupported();

    error IBCPacketCommitmentMismatch(bytes32 expected, bytes32 actual);

    error IBCAppNotFound(string portId);

    error IBCMembershipProofVerificationFailed(IICS26RouterMsgs.Packet packet, ILightClientMsgs.MsgMembership membershipMsg, bytes reason);

    error IBCPacketHandlingFailed(IICS26RouterMsgs.Packet packet, bytes reason);
}
