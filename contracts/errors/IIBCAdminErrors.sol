// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

/// @title IIBCAdminErrors
/// @notice Interface for IBCAdmin contract errors
interface IIBCAdminErrors {
    /// @notice Error returned when caller is not the timelocked admin nor the governance admin
    error Unauthorized();
}
