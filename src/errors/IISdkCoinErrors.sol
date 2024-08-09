// SPDX-License-Identifier: MIT
pragma solidity >=0.8.25;

interface IISdkCoinErrors {
    /// @notice Unsupported token decimals
    /// @param decimals client type
    error UnsupportedTokenDecimals(uint8 decimals);

    /// @notice Invalid address
    /// @param _address address
    error InvalidAddress(address _address);

    /// @notice Invalid token amount
    /// @param amount Amount of tokens being transferred
    error InvalidAmount(uint256 amount);

    /// @notice Thrown when a requested operation or action is not supported by the contract
    error Unsupported();

    ///////////////// Invariant Testing Errors
    /// @param remainder The remainder that should be zero
    error RemainderIsNotZero(uint256 remainder);

    /// @param remainder The remainder that should be greater than zero
    error RemainderIsNotBiggerThanZero(uint256 remainder);

    /// @param convertedAmount The converted amount
    /// @param amount The original amount
    error ConvertedAmountNotEqualInput(uint64 convertedAmount, uint256 amount);

    /// @param convertedAmount The converted amount
    /// @param amount The original amount
    error ConvertedAmountNotBiggerThanOrEqualInput(uint256 convertedAmount, uint64 amount);
}
