// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

// solhint-disable custom-errors,max-line-length

import { Test } from "forge-std/Test.sol";
import { ICSCore } from "../../contracts/ICSCore.sol";
import { IICS04Channel } from "../../contracts/interfaces/IICS04Channel.sol";
import { IICS04ChannelMsgs } from "../../contracts/msgs/IICS04ChannelMsgs.sol";
import { ILightClient } from "../../contracts/interfaces/ILightClient.sol";
import { ILightClientMsgs } from "../../contracts/msgs/ILightClientMsgs.sol";
import { DummyLightClient } from "./mocks/DummyLightClient.sol";

contract ICSCoreTest is Test {
    ICSCore public icsCore;
    DummyLightClient public lightClient;

    bytes[] public merklePrefix = [bytes("ibc"), bytes("")];
    bytes[] public randomPrefix = [bytes("test"), bytes("prefix")];

    string public clientIdentifier;

    function setUp() public {
        lightClient = new DummyLightClient(ILightClientMsgs.UpdateResult.Update, 0, false);
        icsCore = new ICSCore(address(this));

        string memory counterpartyId = "42-dummy-01";
        IICS04ChannelMsgs.Channel memory channel = IICS04ChannelMsgs.Channel(counterpartyId, merklePrefix);
        vm.expectEmit();
        emit IICS04Channel.ICS04ChannelAdded("07-tendermint-0", channel);
        clientIdentifier = icsCore.addChannel("07-tendermint", channel, address(lightClient));

        ILightClient fetchedLightClient = icsCore.getClient(clientIdentifier);
        assertNotEq(address(fetchedLightClient), address(0), "client not found");

        assertEq(channel.counterpartyId, counterpartyId, "channel not set correctly");
    }

    function test_UpdateClient() public {
        bytes memory updateMsg = "testUpdateMsg";
        ILightClient.UpdateResult updateResult = icsCore.updateClient(clientIdentifier, updateMsg);
        assertEq(uint256(updateResult), uint256(ILightClientMsgs.UpdateResult.Update), "updateClient failed");
        assertEq(updateMsg, lightClient.latestUpdateMsg(), "updateClient failed");
    }

    function test_MigrateClient() public {
        string memory counterpartyId = "42-dummy-01";
        DummyLightClient noopLightClient = new DummyLightClient(ILightClientMsgs.UpdateResult.NoOp, 0, false);
        IICS04ChannelMsgs.Channel memory channel = IICS04ChannelMsgs.Channel(counterpartyId, randomPrefix);
        vm.expectEmit();
        emit IICS04Channel.ICS04ChannelAdded("07-tendermint-1", channel);
        string memory substituteIdentifier = icsCore.addChannel("07-tendermint", channel, address(noopLightClient));

        icsCore.migrateClient(clientIdentifier, substituteIdentifier);
        ILightClient fetchedLightClient = icsCore.getClient(clientIdentifier);
        assertEq(address(fetchedLightClient), address(noopLightClient), "client not migrated");

        IICS04Channel.Channel memory fetchedChannel = icsCore.getChannel(clientIdentifier);
        assertEq(fetchedChannel.counterpartyId, counterpartyId, "channel not migrated");
        assertEq(fetchedChannel.merklePrefix, randomPrefix, "channel not migrated");
    }
}
