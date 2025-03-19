// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

interface IIBCUUPSUpgradeableErrors {
    /// @notice Error code returned when caller is not the timelocked admin nor the governance admin
    error Unauthorized();

    /// @notice Default admin role cannot be granted non-programmatically
    error DefaultAdminRoleCannotBeGranted();
}
