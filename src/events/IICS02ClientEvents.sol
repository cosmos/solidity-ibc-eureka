// SPDX-License-Identifier: MIT
pragma solidity >=0.8.25;

import { IICS02ClientMsgs } from "../msgs/IICS02ClientMsgs.sol";

interface IICS02ClientEvents {
    /// @notice Emitted when a new client is added to the client router.
    /// @param clientId The newly created client identifier
    /// @param counterpartyInfo The counterparty client information for the added client
    event ICS02ClientAdded(string clientId, IICS02ClientMsgs.CounterpartyInfo counterpartyInfo);
}
