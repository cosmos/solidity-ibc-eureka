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

contract icsCoreTest is Test {
    ICSCore public icsCore;
    DummyLightClient public lightClient;

    bytes[] public merklePrefix = [bytes("ibc"), bytes("")];

    function setUp() public {
        lightClient = new DummyLightClient(ILightClientMsgs.UpdateResult.Update, 0, false);
        icsCore = new ICSCore(address(this));
    }

    function test_icsCore() public {
        string memory counterpartyId = "42-dummy-01";
        IICS04ChannelMsgs.Channel memory channel = IICS04ChannelMsgs.Channel(counterpartyId, merklePrefix);
        vm.expectEmit();
        emit IICS04Channel.ICS04ChannelAdded("07-tendermint-0", channel);
        string memory clientIdentifier = icsCore.addChannel("07-tendermint", channel, address(lightClient));

        ILightClient fetchedLightClient = icsCore.getClient(clientIdentifier);
        assertNotEq(address(fetchedLightClient), address(0), "client not found");

        assertEq(channel.counterpartyId, counterpartyId, "channel not set correctly");

        bytes memory updateMsg = "testUpdateMsg";
        ILightClient.UpdateResult updateResult = icsCore.updateClient(clientIdentifier, updateMsg);
        assertEq(uint256(updateResult), uint256(ILightClientMsgs.UpdateResult.Update), "updateClient failed");
        assertEq(updateMsg, lightClient.latestUpdateMsg(), "updateClient failed");
    }
}
