// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

/// @title IBCRolesLib
/// @notice This library is used to define the shared roles for IBC contracts.
library IBCRolesLib {
    /// @notice The admin role as per defined by AccessManager.
    uint64 public constant ADMIN_ROLE = type(uint64).min;

    /// @notice The relayer role as per defined by AccessManager.
    uint64 public constant PUBLIC_ROLE = type(uint64).max;

    /// @notice Only addresses with this role may relay packets.
    uint64 public constant RELAYER_ROLE = 1;

    /// @notice The pauser role can pause the ICS20Transfer application.
    uint64 public constant PAUSER_ROLE = 2;

    /// @notice The unpauser role can unpause the ICS20Transfer application.
    uint64 public constant UNPAUSER_ROLE = 3;

    /// @notice Has permission to call `ICS20Transfer.sendTransferWithSender`.
    uint64 public constant DELEGATE_SENDER_ROLE = 4;

    /// @notice Can set withdrawal rate limits per ERC20 token.
    uint64 public constant RATE_LIMITER_ROLE = 5;

    /// @notice Can set custom port ids and client ids in ICS26Router.
    uint64 public constant ID_CUSTOMIZER_ROLE = 6;
    
}
