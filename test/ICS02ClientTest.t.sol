// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

// solhint-disable custom-errors,max-line-length

import { Test } from "forge-std/Test.sol";
import { ICS02Client } from "../src/ICS02Client.sol";
import { IICS04Channel } from "../src/interfaces/IICS04Channel.sol";
import { IICS04ChannelMsgs } from "../src/msgs/IICS04ChannelMsgs.sol";
import { ILightClient } from "../src/interfaces/ILightClient.sol";
import { ILightClientMsgs } from "../src/msgs/ILightClientMsgs.sol";
import { DummyLightClient } from "./mocks/DummyLightClient.sol";

contract ICS02ClientTest is Test {
    ICS02Client public ics02Client;
    DummyLightClient public lightClient;

    bytes[] public merklePrefix = [bytes("ibc"), bytes("")];

    function setUp() public {
        lightClient = new DummyLightClient(ILightClientMsgs.UpdateResult.Update, 0, false);
        ics02Client = new ICS02Client(address(this));
    }

    function test_ICS02Client() public {
        string memory counterpartyId = "42-dummy-01";
        IICS04ChannelMsgs.Channel memory channel =
            IICS04ChannelMsgs.Channel(counterpartyId, merklePrefix);
        vm.expectEmit();
        emit IICS04Channel.ICS04ChannelAdded("07-tendermint-0", channel);
        string memory clientIdentifier = ics02Client.addChannel("07-tendermint", channel, address(lightClient));

        ILightClient fetchedLightClient = ics02Client.getClient(clientIdentifier);
        assertNotEq(address(fetchedLightClient), address(0), "client not found");

        assertEq(channel.counterpartyId, counterpartyId, "channel not set correctly");

        bytes memory updateMsg = "testUpdateMsg";
        ILightClient.UpdateResult updateResult = ics02Client.updateClient(clientIdentifier, updateMsg);
        assertEq(uint256(updateResult), uint256(ILightClientMsgs.UpdateResult.Update), "updateClient failed");
        assertEq(updateMsg, lightClient.latestUpdateMsg(), "updateClient failed");
    }
}
