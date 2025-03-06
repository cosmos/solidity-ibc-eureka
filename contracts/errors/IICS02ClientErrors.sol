// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

interface IICS02ClientErrors {
    /// @notice Invalid client id
    /// @param clientId the invalid client identifier
    error IBCInvalidClientId(string clientId);

    /// @notice Client not found
    /// @param clientId client identifier
    error IBCClientNotFound(string clientId);

    /// @notice Counterparty client not found
    /// @param counterpartyClientId counterparty client identifier
    error IBCCounterpartyClientNotFound(string counterpartyClientId);

    /// @notice IBC client identifier already exists
    /// @param clientId client identifier
    error IBCClientAlreadyExists(string clientId);

    /// @notice Unreachable code
    error Unreachable();
}
