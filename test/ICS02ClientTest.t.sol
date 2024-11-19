// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

// solhint-disable custom-errors,max-line-length

import { Test } from "forge-std/Test.sol";
import { IICS02Client } from "../contracts/interfaces/IICS02Client.sol";
import { ICS02Client } from "../contracts/ICS02Client.sol";
import { IICS02ClientMsgs } from "../contracts/msgs/IICS02ClientMsgs.sol";
import { ILightClient } from "../contracts/interfaces/ILightClient.sol";
import { ILightClientMsgs } from "../contracts/msgs/ILightClientMsgs.sol";
import { DummyLightClient } from "./mocks/DummyLightClient.sol";

contract ICS02ClientTest is Test {
    IICS02Client public ics02Client;
    DummyLightClient public lightClient;

    bytes[] public merklePrefix = [bytes("ibc"), bytes("")];

    function setUp() public {
        lightClient = new DummyLightClient(ILightClientMsgs.UpdateResult.Update, 0, false);
        ics02Client = new ICS02Client(address(this));
    }

    function test_ICS02Client() public {
        string memory counterpartyClient = "42-dummy-01";
        IICS02ClientMsgs.CounterpartyInfo memory counterpartyInfo =
            IICS02ClientMsgs.CounterpartyInfo(counterpartyClient, merklePrefix);
        vm.expectEmit();
        emit IICS02Client.ICS02ClientAdded("07-tendermint-0", counterpartyInfo);
        string memory clientIdentifier = ics02Client.addClient("07-tendermint", counterpartyInfo, address(lightClient));

        ILightClient fetchedLightClient = ics02Client.getClient(clientIdentifier);
        assertNotEq(address(fetchedLightClient), address(0), "client not found");

        assertEq(counterpartyInfo.clientId, counterpartyClient, "counterpartyInfo not found");

        bytes memory updateMsg = "testUpdateMsg";
        ILightClient.UpdateResult updateResult = ics02Client.updateClient(clientIdentifier, updateMsg);
        assertEq(uint256(updateResult), uint256(ILightClientMsgs.UpdateResult.Update), "updateClient failed");
        assertEq(updateMsg, lightClient.latestUpdateMsg(), "updateClient failed");
    }
}
