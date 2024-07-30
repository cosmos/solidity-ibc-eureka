// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.25;

import { IICS02Client } from "./interfaces/IICS02Client.sol";
import { Strings } from "@openzeppelin/contracts/utils/Strings.sol";
import { IBCIdentifiers } from "./utils/IBCIdentifiers.sol";
import { ILightClient } from "./interfaces/ILightClient.sol";
import { IICS02ClientErrors } from "./errors/IICS02ClientErrors.sol";
import { Ownable } from "@openzeppelin/contracts/access/Ownable.sol";

contract ICS02Client is IICS02Client, IICS02ClientErrors, Ownable {
    /// @dev clientId => counterpartyInfo
    mapping(string clientId => CounterpartyInfo info) private counterpartyInfos;
    /// @dev clientId => light client contract
    mapping(string clientId => ILightClient client) private clients;
    /// @dev clientType => nextClientSeq
    mapping(string clientType => uint32 nextClientSeq) private nextClientSeq;

    /// @param owner_ The owner of the contract
    constructor(address owner_) Ownable(owner_) { }

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

    function getCounterparty(string calldata clientId) public view returns (CounterpartyInfo memory) {
        CounterpartyInfo memory counterpartyInfo = counterpartyInfos[clientId];
        if (bytes(counterpartyInfo.clientId).length == 0) {
            revert IBCCounterpartyClientNotFound(clientId);
        }

        return counterpartyInfo;
    }

    function getClient(string calldata clientId) public view returns (ILightClient) {
        ILightClient client = clients[clientId];
        if (client == ILightClient(address(0))) {
            revert IBCClientNotFound(clientId);
        }

        return client;
    }

    function addClient(
        string calldata clientType,
        CounterpartyInfo calldata counterpartyInfo,
        address client
    )
        external
        returns (string memory)
    {
        string memory clientId = getNextClientId(clientType);
        clients[clientId] = ILightClient(client);
        counterpartyInfos[clientId] = counterpartyInfo;
        return clientId;
    }

    function migrateClient(string calldata subjectClientId, string calldata substituteClientId) external onlyOwner {
        getClient(subjectClientId); // Ensure subject client exists
        ILightClient substituteClient = getClient(substituteClientId);

        getCounterparty(subjectClientId); // Ensure subject client's counterparty exists
        CounterpartyInfo memory substituteCounterpartyInfo = getCounterparty(substituteClientId);

        counterpartyInfos[subjectClientId] = substituteCounterpartyInfo;
        clients[subjectClientId] = substituteClient;
    }

    function updateClient(
        string calldata clientId,
        bytes calldata updateMsg
    )
        external
        returns (ILightClient.UpdateResult)
    {
        return getClient(clientId).updateClient(updateMsg);
    }

    function submitMisbehaviour(string calldata clientId, bytes calldata misbehaviourMsg) external {
        getClient(clientId).misbehaviour(misbehaviourMsg);
    }

    function upgradeClient(string calldata clientId, bytes calldata upgradeMsg) external {
        getClient(clientId).upgradeClient(upgradeMsg);
    }
}
