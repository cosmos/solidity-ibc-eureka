// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.28;

interface IICS27GMPErrors {
    /// @notice Invalid address
    /// @param addr Address of the sender or receiver
    error InvalidAddress(string addr);
}
