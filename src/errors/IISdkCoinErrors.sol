// SPDX-License-Identifier: MIT
pragma solidity >=0.8.25;

interface IISdkCoinErrors {
    /// @notice Invalid address
    /// @param _address address
    error InvalidAddress(address _address);

    /// @notice Invalid token amount
    /// @param amount Amount of tokens being transferred
    error InvalidAmount(uint256 amount);

    /// @notice Thrown when a requested operation or action is not supported by the contract
    error Unsupported();
}
