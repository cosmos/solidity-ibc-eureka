// SPDX-License-Identifier: MIT
pragma solidity >=0.8.25;

interface IIErrors {
    /// @param _address client type
    error ZeroAddress(address _address);

    /// @param amount Amount of tokens being transferred
    error ZeroAmountUint256(uint256 amount);

    /// @param amount Amount of tokens being transferred
    error ZeroAmountUint64(uint64 amount);

    /// @notice Thrown when a requested operation or action is not supported by the contract
    error Unsupported();
}
