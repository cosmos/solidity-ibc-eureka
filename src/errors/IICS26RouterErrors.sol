// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.25;

interface IICS26RouterErrors {
    /// @param portId port identifier
    error IBCPortAlreadyExists(string portId);

    /// @param portId port identifier
    error IBCInvalidPortIdentifier(string portId);

    /// @param timeoutTimestamp timeout timestamp in seconds
    error IBCInvalidTimeoutTimestamp(uint32 timeoutTimestamp);
}
