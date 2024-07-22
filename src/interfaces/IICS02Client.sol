// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.25;

import { IICS02ClientMsgs } from "../msgs/IICS02ClientMsgs.sol";
import { ILightClient } from "./ILightClient.sol";

// @title ICS02 Light Client Router Interface
// @notice IICS02Client is an interface for the IBC Eureka client router
interface IICS02Client is IICS02ClientMsgs {
    // @notice Returns the counterparty client information given the client identifier.
    // @param clientId The client identifier
    // @return The counterparty client information
    function getCounterparty(string calldata clientId) external view returns (CounterpartyInfo memory);

    // @notice Returns the creator of the client given the client identifier.
    // @param clientId The client identifier
    // @return The address of the client creator
    function getCreator(string calldata clientId) external view returns (address);

    // @notice Returns the address of the client contract given the client identifier.
    // @param clientId The client identifier
    // @return The address of the client contract
    function getClient(string calldata clientId) external view returns (ILightClient);

    // @notice Adds a client to the client router.
    // @param clientType The client type, e.g., "07-tendermint".
    // @param client The address of the client contract
    // @return The client identifier
    function addClient(string calldata clientType, address client) external returns (string memory);

    // @notice Adds a counterparty to the client router.
    // @param clientId The client identifier
    // @param counterpartyInfo The counterparty client information
    function addCounterparty(string calldata clientId, CounterpartyInfo calldata counterpartyInfo) external;

    // @notice Updates the client given the client identifier.
    // @param clientId The client identifier
    // @param updateMsg The update message
    // @return The result of the update operation
    function routeUpdateClient(
        string calldata clientId,
        bytes calldata updateMsg
    )
        external
        returns (ILightClient.UpdateResult);

    // @notice Submits misbehaviour to the client with the given client identifier.
    // @param clientId The client identifier
    // @param misbehaviourMsg The misbehaviour message
    function routeMisbehaviour(string calldata clientId, bytes calldata misbehaviourMsg) external;
}
