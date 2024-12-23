// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IICS02Client } from "./interfaces/IICS02Client.sol";
import { IICS04Channel } from "./interfaces/IICS04Channel.sol";
import { Strings } from "@openzeppelin/utils/Strings.sol";
import { IBCIdentifiers } from "./utils/IBCIdentifiers.sol";
import { ILightClient } from "./interfaces/ILightClient.sol";
import { IICS02ClientErrors } from "./errors/IICS02ClientErrors.sol";
import { Ownable } from "@openzeppelin/access/Ownable.sol";
import { Initializable } from "@openzeppelin/proxy/utils/Initializable.sol";

contract ICSCore is IICS02Client, IICS04Channel, IICS02ClientErrors, Initializable, Ownable {

    /// @notice Storage of the ICSCore contract
    /// @dev It's implemented on a custom ERC-7201 namespace to reduce the
    /// @dev risk of storage collisions when using with upgradeable contracts.
    /// @param channels Mapping of client identifiers to channels
    /// @param clients Mapping of client identifiers to light client contracts
    /// @param nextClientSeq Mapping of client types to the next client sequence
    /// @custom:storage-location erc7201:cosmos.storage.ICSCore
    struct ICSCoreStorage {
        mapping(string clientId => Channel) channels;
        mapping(string clientId => ILightClient) clients;
        mapping(string clientType => uint32) nextClientSeq;
    }

    /// @notice ERC-7201 slot for the ICSCore storage
    /// @dev keccak256(abi.encode(uint256(keccak256("cosmos.storage.ICSCore")) - 1)) & ~bytes32(uint256(0xff))
    bytes32 private constant ICSCORE_STORAGE_SLOT = 0xd77327ff2954bddd826a8a04ad1c6d0923c50f3ddca6ff0b4d10223afaa23000;

    /// @dev This contract is meant to be deployed by a proxy, so the constructor is not used
    constructor() Ownable(address(0xdead)) {
        _disableInitializers();
    }

    /// @notice Initializes the contract instead of a constructor
    /// @dev Meant to be called only once from the proxy
    /// @param owner_ The owner of the contract
    function initialize(address owner_) public initializer {
        _transferOwnership(owner_);
    }

    /// @notice Generates the next client identifier
    /// @param clientType The client type
    /// @return The next client identifier
    function getNextClientId(string calldata clientType) private returns (string memory) {
        ICSCoreStorage storage $ = _getICSCoreStorage();

        require(IBCIdentifiers.validateClientType(clientType), IBCInvalidClientType(clientType));

        uint32 seq = $.nextClientSeq[clientType];
        $.nextClientSeq[clientType] = seq + 1;
        return string.concat(clientType, "-", Strings.toString(seq));
    }

    /// @inheritdoc IICS04Channel
    function getChannel(string calldata channelId) public view returns (Channel memory) {
        Channel memory channel = _getICSCoreStorage().channels[channelId];
        require(bytes(channel.counterpartyId).length != 0, IBCCounterpartyClientNotFound(channelId));

        return channel;
    }

    /// @inheritdoc IICS02Client
    function getClient(string calldata clientId) public view returns (ILightClient) {
        ILightClient client = _getICSCoreStorage().clients[clientId];
        require(address(client) != address(0), IBCClientNotFound(clientId));

        return client;
    }

    /// @inheritdoc IICS04Channel
    function addChannel(
        string calldata clientType,
        Channel calldata channel,
        address client
    )
        external
        returns (string memory)
    {
        ICSCoreStorage storage $ = _getICSCoreStorage();

        string memory clientId = getNextClientId(clientType);
        $.clients[clientId] = ILightClient(client);
        // use the same identifier for channelId and clientId
        // for Solidity implementation
        $.channels[clientId] = channel;

        emit ICS04ChannelAdded(clientId, channel);

        return clientId;
    }

    /// @inheritdoc IICS02Client
    function migrateClient(string calldata subjectClientId, string calldata substituteClientId) external onlyOwner {
        ICSCoreStorage storage $ = _getICSCoreStorage();

        getClient(subjectClientId); // Ensure subject client exists
        ILightClient substituteClient = getClient(substituteClientId);

        getChannel(subjectClientId); // Ensure channel exists for this clientId
        Channel memory substituteChannel = getChannel(substituteClientId);

        $.channels[subjectClientId] = substituteChannel;
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
}
