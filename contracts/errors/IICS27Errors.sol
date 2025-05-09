// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.28;

interface IICS27Errors {
    /// @notice Invalid address
    /// @param addr Address of the sender or receiver
    error ICS27InvalidAddress(string addr);

    /// @notice Unauthorized function call
    /// @param expected The expected address
    /// @param caller The caller of the function
    error ICS27Unauthorized(address expected, address caller);

    /// @notice Unexpected packet data version
    /// @param expected expected version of the packet data
    /// @param version actual version of the packet data
    error ICS27UnexpectedVersion(string expected, string version);

    /// @notice Unexpected packet data encoding
    /// @param expected expected encoding of the packet data
    /// @param actual actual encoding of the packet data
    error ICS27UnexpectedEncoding(string expected, string actual);

    /// @notice Invalid port
    /// @param expected Expected port
    /// @param actual Actual port
    error ICS27InvalidPort(string expected, string actual);

    /// @notice Invalid receiver
    /// @param receiver The receiver address of the contract call
    error ICS27InvalidReceiver(string receiver);
}
