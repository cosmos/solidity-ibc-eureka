// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

interface IIBCUpgradeableBeaconErrors {
    /// @dev The `implementation` of the beacon is invalid.
    error BeaconInvalidImplementation(address implementation);

    /// @dev The sender is not authorized to update the implementation.
    error Unauthorized(address sender);
}
