// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

interface IIBCUUPSUpgradeable {
    /// @notice Returns the multisig admin address
    /// @return The multisig admin address
    function getMultisigAdmin() external view returns (address);
    /// @notice Returns the governance admin address
    /// @return The governance admin address, 0 if not set
    function getGovAdmin() external view returns (address);
    /// @notice Adds a governance admin address
    /// @dev Can only be called by the multisig admin once.
    /// @dev The multisigAdmin should be timelocked after setting the govAdmin.
    /// @param govAdmin The address of the governance admin
    function addGovAdmin(address govAdmin) external;
    /// @notice Changes the multisig admin address
    /// @dev Either admin can change the multisig admin address.
    /// @param newMultisigAdmin The new multisig admin address
    function changeMultisigAdmin(address newMultisigAdmin) external;
}
