// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

interface IIBCPausableUpgradeableErrors {
    /// @notice Error code returned when caller is not the pauser
    error Unauthorized();
}
