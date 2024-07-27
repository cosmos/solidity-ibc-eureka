// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.25;

import { IICS02ClientMsgs } from "../msgs/IICS02ClientMsgs.sol";
import { ILightClient } from "./ILightClient.sol";

/// @title ICS02 Light Client Router Interface
/// @notice IICS02Client is an interface for the IBC Eureka client router
interface IICS02Client is IICS02ClientMsgs {
    /// @notice Returns the counterparty client information given the client identifier.
    /// @param clientId The client identifier
    /// @return The counterparty client information
    function getCounterparty(string calldata clientId) external view returns (CounterpartyInfo memory);

    /// @notice Returns the address of the client contract given the client identifier.
    /// @param clientId The client identifier
    /// @return The address of the client contract
    function getClient(string calldata clientId) external view returns (ILightClient);

    /// @notice Adds a client to the client router.
    /// @param clientType The client type, e.g., "07-tendermint".
    /// @param counterpartyInfo The counterparty client information
    /// @param client The address of the client contract
    /// @return The client identifier
    function addClient(
        string calldata clientType,
        CounterpartyInfo calldata counterpartyInfo,
        address client
    )
        external
        returns (string memory);

    /// @notice Migrate the underlying client of the subject client to the substitute client.
    /// @dev This is a privilaged operation, only the owner of ICS02Client can call this function.
    /// @param subjectClientId The client identifier of the subject client
    /// @param substituteClientId The client identifier of the substitute client
    function migrateClient(string calldata subjectClientId, string calldata substituteClientId) external;

    /// @notice Updates the client given the client identifier.
    /// @param clientId The client identifier
    /// @param updateMsg The update message
    /// @return The result of the update operation
    function updateClient(
        string calldata clientId,
        bytes calldata updateMsg
    )
        external
        returns (ILightClient.UpdateResult);

    /// @notice Submits misbehaviour to the client with the given client identifier.
    /// @param clientId The client identifier
    /// @param misbehaviourMsg The misbehaviour message
    function submitMisbehaviour(string calldata clientId, bytes calldata misbehaviourMsg) external;

    /// @notice Upgrades the client with the given client identifier.
    /// @param clientId The client identifier
    /// @param upgradeMsg The upgrade message
    function upgradeClient(string calldata clientId, bytes calldata upgradeMsg) external;
}
