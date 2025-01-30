// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

interface IIBCUUPSUpgradeable {
    /// @notice Returns the multisig admin address
    /// @return The multisig admin address
    function getMultisigAdmin() external view returns (address);
    /// @notice Returns the governance admin address
    /// @return The governance admin address, 0 if not set
    function getGovAdmin() external view returns (address);
    /// @notice Sets the multisig admin address
    /// @dev Either admin can set the multisig admin address.
    /// @param newMultisigAdmin The new multisig admin address
    function setMultisigAdmin(address newMultisigAdmin) external;
    /// @notice Sets the governance admin address
    /// @dev Either admin can set the governance admin address.
    /// @dev Since multisig admin is timelocked, this operation can be stopped by the govAdmin.
    /// @param newGovAdmin The new governance admin address
    function setGovAdmin(address newGovAdmin) external;
}
