// SPDX-License-Identifier: MIT
pragma solidity >=0.8.25;

interface IISdkCoinErrors {
    /// @param decimals client type
    error UnsupportedTokenDecimals(uint8 decimals);

    /// @param _address address
    error ZeroAddress(address _address);

    /// @param amount Amount of tokens being transferred
    error ZeroAmountUint256(uint256 amount);

    /// @param amount Amount of tokens being transferred
    error ZeroAmountUint64(uint64 amount);

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
