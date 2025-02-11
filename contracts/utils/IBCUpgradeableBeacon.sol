// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

/**
    This contract is a modified version of OpenZeppelin's UpgradeableBeacon.sol

    https://github.com/OpenZeppelin/openzeppelin-contracts/blob/f4237626c2e107f120783a15e7820e60bc73b2d8/contracts/proxy/beacon/UpgradeableBeacon.sol#L2
*/

import { IBeacon } from "@openzeppelin-contracts/proxy/beacon/IBeacon.sol";
import { IIBCUUPSUpgradeable } from "../interfaces/IIBCUUPSUpgradeable.sol";

contract IBCUpgradeableBeacon is IBeacon {
    address private _implementation;
    address private _ics26;

    /// @dev The `implementation` of the beacon is invalid.
    error BeaconInvalidImplementation(address implementation);

    /// @dev The sender is not authorized to update the implementation.
    error Unauthorized(address sender);

    /// @dev Emitted when the implementation returned by the beacon is changed.
    event Upgraded(address indexed implementation);

    /// @dev Sets the address of the initial implementation, and the initial owner who can upgrade the beacon.
    constructor(address implementation_, address ics26_) {
        _setImplementation(implementation_);
        _ics26 = ics26_;
    }

    /// @dev Returns the current implementation address.
    function implementation() public view virtual returns (address) {
        return _implementation;
    }

    /**
     * @dev Sets the implementation contract address for this beacon
     *
     * Requirements:
     *
     * - `newImplementation` must be a contract.
     */
    function _setImplementation(address newImplementation) private {
        require(IIBCUUPSUpgradeable(_ics26).isAdmin(msg.sender), Unauthorized(msg.sender));
        if (newImplementation.code.length == 0) {
            revert BeaconInvalidImplementation(newImplementation);
        }
        _implementation = newImplementation;
        emit Upgraded(newImplementation);
    }
}
