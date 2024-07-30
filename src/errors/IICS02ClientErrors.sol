// SPDX-License-Identifier: MIT
pragma solidity >=0.8.25;

interface IICS02ClientErrors {
    /// @param clientType client type
    error IBCInvalidClientType(string clientType);

    /// @param clientId client identifier
    error IBCClientNotFound(string clientId);

    /// @param counterpartyClientId counterparty client identifier
    error IBCCounterpartyClientNotFound(string counterpartyClientId);
}
