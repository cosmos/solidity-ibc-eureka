// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IICS02Client } from "../interfaces/IICS02Client.sol";
import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";
import { IBCIdentifiers } from "../utils/IBCIdentifiers.sol";
import { ILightClient } from "../interfaces/ILightClient.sol";
import { IICS02ClientErrors } from "../errors/IICS02ClientErrors.sol";
import { AccessControlUpgradeable } from "@openzeppelin-upgradeable/access/AccessControlUpgradeable.sol";

/// @title ICS02 Client contract
/// @notice This contract implements the ICS02 Client Router interface
/// @dev Light client migrations/upgrades are supported via `AccessControl` role-based access control
/// @dev Each client is identified by a unique identifier, hash of which also serves as the role identifier
/// @dev The light client migrator role is granted to whoever called `addClient` for the client, and can be revoked (not
/// transferred)
abstract contract ICS02ClientUpgradeable is IICS02Client, IICS02ClientErrors, AccessControlUpgradeable {
    /// @notice Storage of the ICS02Client contract
    /// @dev It's implemented on a custom ERC-7201 namespace to reduce the
    /// @dev risk of storage collisions when using with upgradeable contracts.
    /// @param clients Mapping of client identifiers to light client contracts
    /// @param counterpartyInfos Mapping of client identifiers to counterparty info
    /// @param nextClientSeq The next sequence number for the next client identifier
    /// @custom:storage-location erc7201:ibc.storage.ICS02Client
    struct ICS02ClientStorage {
        mapping(string clientId => ILightClient) clients;
        mapping(string clientId => CounterpartyInfo info) counterpartyInfos;
        uint32 nextClientSeq;
    }

    /// @notice ERC-7201 slot for the ICS02Client storage
    /// @dev keccak256(abi.encode(uint256(keccak256("ibc.storage.ICS02Client")) - 1)) & ~bytes32(uint256(0xff))
    bytes32 private constant ICS02CLIENT_STORAGE_SLOT =
        0x515a8336edcaab4ae6524d41223c1782132890f89189ba6632107a7b5a449600;

    /// @notice Prefix for the light client migrator roles
    /// @dev The role identifier is driven in _getLightClientMigratorRole
    string private constant MIGRATOR_ROLE_PREFIX = "LIGHT_CLIENT_MIGRATOR_ROLE_";

    // no need to run any initialization logic
    // solhint-disable-next-line no-empty-blocks
    function __ICS02Client_init() internal onlyInitializing { }

    /// @notice Generates the next client identifier
    /// @param clientType The client type
    /// @return The next client identifier
    function getNextClientId(string calldata clientType) private returns (string memory) {
        ICS02ClientStorage storage $ = _getICS02ClientStorage();

        require(IBCIdentifiers.validateClientType(clientType), IBCInvalidClientType(clientType));

        uint32 seq = $.nextClientSeq;
        $.nextClientSeq = seq + 1;
        return string.concat(clientType, "-", Strings.toString(seq));
    }

    /// @inheritdoc IICS02Client
    function getCounterparty(string calldata clientId) public view returns (CounterpartyInfo memory) {
        CounterpartyInfo memory counterpartyInfo = _getICS02ClientStorage().counterpartyInfos[clientId];
        require(bytes(counterpartyInfo.clientId).length != 0, IBCCounterpartyClientNotFound(clientId));

        return counterpartyInfo;
    }

    /// @inheritdoc IICS02Client
    function getClient(string calldata clientId) public view returns (ILightClient) {
        ILightClient client = _getICS02ClientStorage().clients[clientId];
        require(address(client) != address(0), IBCClientNotFound(clientId));

        return client;
    }

    /// @inheritdoc IICS02Client
    function addClient(
        string calldata clientType,
        CounterpartyInfo calldata counterpartyInfo,
        address client
    )
        external
        returns (string memory)
    {
        ICS02ClientStorage storage $ = _getICS02ClientStorage();

        string memory clientId = getNextClientId(clientType);
        $.clients[clientId] = ILightClient(client);
        $.counterpartyInfos[clientId] = counterpartyInfo;

        emit ICS02ClientAdded(clientId, counterpartyInfo);

        bytes32 role = _getLightClientMigratorRole(clientId);
        require(_grantRole(role, _msgSender()), Unreachable());

        return clientId;
    }

    /// @inheritdoc IICS02Client
    function migrateClient(
        string calldata subjectClientId,
        string calldata substituteClientId
    )
        external
        onlyRole(_getLightClientMigratorRole(subjectClientId))
    {
        ICS02ClientStorage storage $ = _getICS02ClientStorage();

        getClient(subjectClientId); // Ensure subject client exists
        ILightClient substituteClient = getClient(substituteClientId);

        getCounterparty(subjectClientId); // Ensure subject client's counterparty exists
        CounterpartyInfo memory substituteCounterpartyInfo = getCounterparty(substituteClientId);

        $.counterpartyInfos[subjectClientId] = substituteCounterpartyInfo;
        $.clients[subjectClientId] = substituteClient;
    }

    /// @inheritdoc IICS02Client
    function updateClient(
        string calldata clientId,
        bytes calldata updateMsg
    )
        external
        returns (ILightClient.UpdateResult)
    {
        return getClient(clientId).updateClient(updateMsg);
    }

    /// @inheritdoc IICS02Client
    function submitMisbehaviour(string calldata clientId, bytes calldata misbehaviourMsg) external {
        getClient(clientId).misbehaviour(misbehaviourMsg);
    }

    /// @inheritdoc IICS02Client
    function upgradeClient(string calldata clientId, bytes calldata upgradeMsg) external {
        getClient(clientId).upgradeClient(upgradeMsg);
    }

    /// @notice Returns the storage of the ICS02Client contract
    function _getICS02ClientStorage() private pure returns (ICS02ClientStorage storage $) {
        // solhint-disable-next-line no-inline-assembly
        assembly {
            $.slot := ICS02CLIENT_STORAGE_SLOT
        }
    }

    /// @notice Returns the role identifier for a light client
    /// @param clientId The client identifier
    /// @return The role identifier
    function _getLightClientMigratorRole(string memory clientId) private pure returns (bytes32) {
        return keccak256(abi.encodePacked(MIGRATOR_ROLE_PREFIX, clientId));
    }
}
