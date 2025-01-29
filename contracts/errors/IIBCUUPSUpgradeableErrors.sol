// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

interface IIBCUUPSUpgradeableErrors {
    /// @notice Error code returned when caller is not the multisig admin nor the governance admin
    error Unauthorized();

    /// @notice Governance admin has already been set
    error GovernanceAdminAlreadySet();

    /// @notice Caller is not the governance admin
    error CallerNotGovernanceAdmin();

    /// @notice Caller is not the multisig admin
    error CallerNotMultisigAdmin();
}
