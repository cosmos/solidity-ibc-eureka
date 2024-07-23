// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.25;

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
}
