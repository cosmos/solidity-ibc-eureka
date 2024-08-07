// SPDX-License-Identifier: MIT
pragma solidity >=0.8.25;

interface IISdkCoinErrors {
    /// @param decimals client type
    error UnsupportedTokenDecimals(uint8 decimals);
}
