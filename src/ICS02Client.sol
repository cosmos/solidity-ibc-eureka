// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.25;

import { IICS02Client } from "./interfaces/IICS02Client.sol";
import { Strings } from "@openzeppelin/contracts/utils/Strings.sol";
import { IBCIdentifiers } from "./utils/IBCIdentifiers.sol";
import { ILightClient } from "./interfaces/ILightClient.sol";
import { IICS02ClientErrors } from "./errors/IICS02ClientErrors.sol";

contract ICS02Client is IICS02Client, IICS02ClientErrors {
    /// @dev clientId => counterpartyInfo
    mapping(string => CounterpartyInfo) private counterpartyInfos;
    /// @dev clientId => client
    mapping(string => ILightClient) private clients;
    /// @dev clientType => nextClientSeq
    mapping(string => uint32) private nextClientSeq;

    /// @notice Generates the next client identifier
    /// @param clientType The client type
    /// @return The next client identifier
    function getNextClientId(string calldata clientType) private returns (string memory) {
        if (!IBCIdentifiers.validateClientType(clientType)) {
            revert IBCInvalidClientType(clientType);
        }

        uint32 seq = nextClientSeq[clientType];
        nextClientSeq[clientType] = seq + 1;
        return string.concat(clientType, "-", Strings.toString(seq));
    }

    function getCounterparty(string calldata clientId) external view returns (CounterpartyInfo memory) {
        return counterpartyInfos[clientId];
    }

    function getClient(string calldata clientId) external view returns (ILightClient) {
        return clients[clientId];
    }

    function addClient(string calldata clientType, CounterpartyInfo calldata counterpartyInfo, address client) external returns (string memory) {
        string memory clientId = getNextClientId(clientType);
        clients[clientId] = ILightClient(client);
        counterpartyInfos[clientId] = counterpartyInfo;
        return clientId;
    }

    function updateClient(string calldata clientId, bytes calldata updateMsg) external returns (ILightClient.UpdateResult) {
        return clients[clientId].updateClient(updateMsg);
    }

    function submitMisbehaviour(string calldata clientId, bytes calldata misbehaviourMsg) external {
        clients[clientId].misbehaviour(misbehaviourMsg);
    }
}
