// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IICS02Client } from "./interfaces/IICS02Client.sol";
import { IICS04Channel } from "./interfaces/IICS04Channel.sol";
import { Strings } from "@openzeppelin/utils/Strings.sol";
import { IBCIdentifiers } from "./utils/IBCIdentifiers.sol";
import { ILightClient } from "./interfaces/ILightClient.sol";
import { IICS02ClientErrors } from "./errors/IICS02ClientErrors.sol";
import { Ownable } from "@openzeppelin/access/Ownable.sol";
import { Pausable } from "@openzeppelin/utils/Pausable.sol";

contract ICSCore is IICS02Client, IICS04Channel, IICS02ClientErrors, Ownable, Pausable {
    /// @dev channelId => channel
    mapping(string channelId => Channel channel) private channels;
    /// @dev clientId => light client contract
    mapping(string clientId => ILightClient client) private clients;
    /// @dev clientType => nextClientSeq
    mapping(string clientType => uint32 nextClientSeq) private nextClientSeq;

    address private immutable SAFE_ADDRESS;

     constructor(address _safeAddress) Ownable(address(0xdead)) {
        SAFE_ADDRESS = _safeAddress; //  This should not be passed as input but instead Should be an hardcoded constant to be set after safe multisig deployment and before this contracts gets deployed. 
        // Setting now in input for easy testing. 
    }

    function initialize(address _safeAddress) external {
        require(owner() == address(0), "Already initialized");
        require(_safeAddress == SAFE_ADDRESS, "Only Safe can initialize");
    
        _transferOwnership(SAFE_ADDRESS); // Transfer ownership to Safe
    }

    /// @notice Generates the next client identifier
    /// @param clientType The client type
    /// @return The next client identifier
    function getNextClientId(string calldata clientType) private returns (string memory) {
        require(IBCIdentifiers.validateClientType(clientType), IBCInvalidClientType(clientType));

        uint32 seq = nextClientSeq[clientType];
        nextClientSeq[clientType] = seq + 1;
        return string.concat(clientType, "-", Strings.toString(seq));
    }

    /// @inheritdoc IICS04Channel
    function getChannel(string calldata channelId) public view returns (Channel memory) {
        Channel memory channel = channels[channelId];
        require(bytes(channel.counterpartyId).length != 0, IBCCounterpartyClientNotFound(channelId));

        return channel;
    }

    /// @inheritdoc IICS02Client
    function getClient(string calldata clientId) public view returns (ILightClient) {
        ILightClient client = clients[clientId];
        require(address(client) != address(0), IBCClientNotFound(clientId));

        return client;
    }

    /// @inheritdoc IICS04Channel
    function addChannel(
        string calldata clientType,
        Channel calldata channel,
        address client
    )
        external whenNotPaused
        returns (string memory)
    {
        string memory clientId = getNextClientId(clientType);
        clients[clientId] = ILightClient(client);
        // use the same identifier for channelId and clientId
        // for Solidity implementation
        channels[clientId] = channel;

        emit ICS04ChannelAdded(clientId, channel);

        return clientId;
    }

    /// @inheritdoc IICS02Client
    function migrateClient(string calldata subjectClientId, string calldata substituteClientId) external onlyOwner whenNotPaused {
        getClient(subjectClientId); // Ensure subject client exists
        ILightClient substituteClient = getClient(substituteClientId);

        getChannel(subjectClientId); // Ensure channel exists for this clientId
        Channel memory substituteChannel = getChannel(substituteClientId);

        channels[subjectClientId] = substituteChannel;
        clients[subjectClientId] = substituteClient;
    }

    /// @inheritdoc IICS02Client
    function updateClient(
        string calldata clientId,
        bytes calldata updateMsg
    )
        external whenNotPaused
        returns (ILightClient.UpdateResult)
    {
        return getClient(clientId).updateClient(updateMsg);
    }

    /// @inheritdoc IICS02Client
    function submitMisbehaviour(string calldata clientId, bytes calldata misbehaviourMsg) external whenNotPaused{
        getClient(clientId).misbehaviour(misbehaviourMsg);
    }

    /// @inheritdoc IICS02Client
    function upgradeClient(string calldata clientId, bytes calldata upgradeMsg) external whenNotPaused{
        getClient(clientId).upgradeClient(upgradeMsg);
    }
}
