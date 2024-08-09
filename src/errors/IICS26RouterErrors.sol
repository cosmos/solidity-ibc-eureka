// SPDX-License-Identifier: MIT
pragma solidity >=0.8.25;

interface IICS26RouterErrors {
    /// @notice IBC port identifier already exists
    /// @param portId port identifier
    error IBCPortAlreadyExists(string portId);

    /// @notice IBC invalid port identifier
    /// @param portId port identifier
    error IBCInvalidPortIdentifier(string portId);

    /// @notice IBC invalid timeout timestamp
    /// @param timeoutTimestamp packet's timeout timestamp in seconds
    /// @param comparedTimestamp compared timestamp in seconds
    error IBCInvalidTimeoutTimestamp(uint256 timeoutTimestamp, uint256 comparedTimestamp);

    /// @notice IBC unexpected counterparty identifier
    /// @param expected expected counterparty identifier
    /// @param actual actual counterparty identifier
    error IBCInvalidCounterparty(string expected, string actual);

    /// @notice IBC async acknowledgement not supported
    error IBCAsyncAcknowledgementNotSupported();

    /// @notice IBC packet commitment mismatch
    /// @param expected stored packet commitment
    /// @param actual actual packet commitment
    error IBCPacketCommitmentMismatch(bytes32 expected, bytes32 actual);

    /// @notice IBC app for port not found
    /// @param portId port identifier
    error IBCAppNotFound(string portId);
}
