// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.28;

interface IICS27Errors {
    /// @notice Invalid address
    /// @param addr Address of the sender or receiver
    error InvalidAddress(string addr);

    /// @notice Unauthorized function call
    /// @param expected The expected address
    /// @param caller The caller of the function
    error Unauthorized(address expected, address caller);
}
