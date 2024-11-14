// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { IICS04ChannelMsgs } from "../msgs/IICS04ChannelMsgs.sol";

/// @title ICS04 Channel Interface
/// @notice IICS04CHANNEL is an interface for the IBC Eureka channel router
interface IICS04Channel is IICS04ChannelMsgs {
    /// @notice Emitted when a new channel is added to the channel router.
    /// @param channelId The newly created channel identifier. NOTE: In this implementation, the channelId is the client identifier.
    /// @param channel The counterparty client information, if provided
    event ICS04ChannelAdded(string channelId, Channel channel);

    /// @notice Returns the channel given the channel identifier.
    /// @param channelId The channel identifier
    /// @return channel
    function getChannel(string calldata channelId) external view returns (Channel memory);

    /// @notice Adds a channel to the channel router.
    /// @param clientType The client type, e.g., "07-tendermint".
    /// @param channel The channel information
    /// @param client The address of the client contract
    /// @return The channel identifier NOTE: This is the same as the client identifier
    function addChannel(
        string calldata clientType,
        Channel calldata channel,
        address client
    )
        external
        returns (string memory);
}