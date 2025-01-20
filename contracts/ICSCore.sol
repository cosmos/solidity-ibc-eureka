// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IICS02Client } from "./interfaces/IICS02Client.sol";
import { Strings } from "@openzeppelin/utils/Strings.sol";
import { IBCIdentifiers } from "./utils/IBCIdentifiers.sol";
import { ILightClient } from "./interfaces/ILightClient.sol";
import { IICS02ClientErrors } from "./errors/IICS02ClientErrors.sol";
import { Ownable } from "@openzeppelin/access/Ownable.sol";
import { Initializable } from "@openzeppelin/proxy/utils/Initializable.sol";
import { AccessControl } from "@openzeppelin/access/AccessControl.sol";

/// @title ICSCore contract
/// @notice This contract implements the ICS02 Client Router and ICS04 Channel Keeper interfaces
/// @dev Light client migrations/upgrades are supported via `AccessControl` role-based access control
/// @dev Each client is identified by a unique identifier, hash of which also serves as the role identifier
/// @dev The light client role is granted to whoever called `addChannel` for the client, and can be revoked (not
/// transferred)
contract ICSCore is IICS02Client, IICS02ClientErrors, Initializable, Ownable, AccessControl {
    /// @notice Storage of the ICSCore contract
    /// @dev It's implemented on a custom ERC-7201 namespace to reduce the
    /// @dev risk of storage collisions when using with upgradeable contracts.
    /// @param clients Mapping of client identifiers to light client contracts
    /// @param counterpartyInfos Mapping of client identifiers to counterparty info
    /// @param nextClientSeq The next sequence number for the next client identifier
    /// @custom:storage-location erc7201:ibc.storage.ICSCore
    struct ICSCoreStorage {
        mapping(string clientId => ILightClient) clients;
        mapping(string clientId => CounterpartyInfo info) counterpartyInfos;
        uint32 nextClientSeq;
    }

    /// @notice ERC-7201 slot for the ICSCore storage
    /// @dev keccak256(abi.encode(uint256(keccak256("ibc.storage.ICSCore")) - 1)) & ~bytes32(uint256(0xff))
    bytes32 private constant ICSCORE_STORAGE_SLOT = 0x96c0fa34415d0022ef5b75a694f23f508dd3f8a3506b45247b4c4b205af19a00;

    /// @dev This contract is meant to be deployed by a proxy, so the constructor is not used
    constructor() Ownable(address(0xdead)) {
        _disableInitializers();
    }

    /// @notice Initializes the contract instead of a constructor
    /// @dev Meant to be called only once from the proxy
    /// @param owner_ The owner of the contract
    function initialize(address owner_) public initializer {
        _transferOwnership(owner_);
        _grantRole(DEFAULT_ADMIN_ROLE, owner_);
    }

    /// @notice Generates the next client identifier
    /// @param clientType The client type
    /// @return The next client identifier
    function getNextClientId(string calldata clientType) private returns (string memory) {
        ICSCoreStorage storage $ = _getICSCoreStorage();

        require(IBCIdentifiers.validateClientType(clientType), IBCInvalidClientType(clientType));

        uint32 seq = $.nextClientSeq;
        $.nextClientSeq = seq + 1;
        return string.concat(clientType, "-", Strings.toString(seq));
    }

    /// @inheritdoc IICS02Client
    function getCounterparty(string calldata clientId) public view returns (CounterpartyInfo memory) {
        CounterpartyInfo memory counterpartyInfo = _getICSCoreStorage().counterpartyInfos[clientId];
        require(bytes(counterpartyInfo.clientId).length != 0, IBCCounterpartyClientNotFound(clientId));

        return counterpartyInfo;
    }

    /// @inheritdoc IICS02Client
    function getClient(string calldata clientId) public view returns (ILightClient) {
        ILightClient client = _getICSCoreStorage().clients[clientId];
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
        ICSCoreStorage storage $ = _getICSCoreStorage();

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
        ICSCoreStorage storage $ = _getICSCoreStorage();

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

    /// @notice Returns the storage of the ICSCore contract
    function _getICSCoreStorage() private pure returns (ICSCoreStorage storage $) {
        // solhint-disable-next-line no-inline-assembly
        assembly {
            $.slot := ICSCORE_STORAGE_SLOT
        }
    }

    /// @notice Returns the role identifier for a light client
    /// @param clientId The client identifier
    /// @return The role identifier
    function _getLightClientMigratorRole(string memory clientId) private pure returns (bytes32) {
        return keccak256(bytes(clientId));
    }
}
