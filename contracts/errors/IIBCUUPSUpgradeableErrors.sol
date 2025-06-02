// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

/// @title IIBCUUPSUpgradeableErrors
/// @notice Interface for IBCUUPSUpgradeable errors
interface IIBCUUPSUpgradeableErrors {
    /// @notice Error code returned when caller is not the timelocked admin nor the governance admin
    error Unauthorized();
}
