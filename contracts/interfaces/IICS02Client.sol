// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IICS02ClientMsgs } from "../msgs/IICS02ClientMsgs.sol";
import { ILightClient } from "./ILightClient.sol";

/// @title ICS02 Light Client Router Interface
/// @notice IICS02Client is an interface for the IBC Eureka client router
interface IICS02Client {
    /// @notice The role identifier for the client id customizer role
    /// @dev The client identifier role is used to add IBC clients with custom client identifiers
    /// @return The role identifier
    function CLIENT_ID_CUSTOMIZER_ROLE() external view returns (bytes32);

    /// @notice Returns the counterparty client information given the client identifier.
    /// @param clientId The client identifier
    /// @return The counterparty client information
    function getCounterparty(string calldata clientId)
        external
        view
        returns (IICS02ClientMsgs.CounterpartyInfo memory);

    /// @notice Returns the address of the client contract given the client identifier.
    /// @param clientId The client identifier
    /// @return The address of the client contract
    function getClient(string calldata clientId) external view returns (ILightClient);

    /// @notice Returns the next client sequence number.
    /// @dev This function can be used to determine when to stop iterating over clients.
    /// @return The next client sequence number
    function getNextClientSeq() external view returns (uint256);

    /// @notice Adds a client to the client router.
    /// @param counterpartyInfo The counterparty client information
    /// @param client The address of the client contract
    /// @return The client identifier
    function addClient(
        IICS02ClientMsgs.CounterpartyInfo calldata counterpartyInfo,
        address client
    )
        external
        returns (string memory);

    /// @notice Adds a client to the client router.
    /// @dev Only a caller with `CLIENT_ID_CUSTOMIZER_ROLE` can call this function.
    /// @param clientId The custom client identifier
    /// @param counterpartyInfo The counterparty client information
    /// @param client The address of the client contract
    /// @return The client identifier
    function addClient(
        string memory clientId,
        IICS02ClientMsgs.CounterpartyInfo calldata counterpartyInfo,
        address client
    )
        external
        returns (string memory);

    /// @notice Migrate the underlying client of the subject client to the substitute client.
    /// @dev This is a privilaged operation, only the owner of ICS02Client can call this function.
    /// @param subjectClientId The client identifier of the subject client
    /// @param substituteClientId The client identifier of the substitute client
    function migrateClient(string calldata subjectClientId, string calldata substituteClientId) external;

    /// @notice Submits misbehaviour to the client with the given client identifier.
    /// @param clientId The client identifier
    /// @param misbehaviourMsg The misbehaviour message
    function submitMisbehaviour(string calldata clientId, bytes calldata misbehaviourMsg) external;

    /// @notice Returns the role identifier for a light client
    /// @param clientId The client identifier
    /// @return The role identifier
    function getLightClientMigratorRole(string memory clientId) external view returns (bytes32);

    // ============ Events ============

    /// @notice Emitted when a new client is added to the client router.
    /// @param clientId The newly created client identifier
    /// @param counterpartyInfo The counterparty client information, if provided
    event ICS02ClientAdded(string clientId, IICS02ClientMsgs.CounterpartyInfo counterpartyInfo);

    /// @notice Emitted when a client is migrated to a new client.
    /// @param subjectClientId The client identifier of the existing client
    /// @param substituteClientId The client identifier of the new client migrated to
    event ICS02ClientMigrated(string subjectClientId, string substituteClientId);

    /// @notice Emitted when a misbehaviour is submitted to a client and the client is frozen.
    /// @param clientId The client identifier of the frozen client
    /// @param misbehaviourMsg The misbehaviour message
    event ICS02MisbehaviourSubmitted(string clientId, bytes misbehaviourMsg);
}
