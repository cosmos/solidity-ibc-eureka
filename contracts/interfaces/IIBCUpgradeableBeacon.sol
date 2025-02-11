// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

interface IIBCUpgradeableBeacon {
    /// @notice Returns the ICS26 contract address.
    /// @return The ICS26 contract address
    function ics26() external view returns (address);

    /**
     * @dev Upgrades the beacon to a new implementation.
     *
     * Emits an {Upgraded} event.
     *
     * Requirements:
     *
     * - msg.sender must be the owner of the contract.
     * - `newImplementation` must be a contract.
     */
    function upgradeTo(address newImplementation) external;

    // --------------------- Events --------------------- //

    /// @dev Emitted when the implementation returned by the beacon is changed.
    event Upgraded(address indexed implementation);
}
