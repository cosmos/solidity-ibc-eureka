// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.25 <0.9.0;

// solhint-disable custom-errors,max-line-length

import { Test } from "forge-std/src/Test.sol";
import { IICS02Client } from "../src/interfaces/IICS02Client.sol";
import { ICS02Client } from "../src/ICS02Client.sol";
import { IICS02ClientMsgs } from "../src/msgs/IICS02ClientMsgs.sol";
import { ILightClient } from "../src/interfaces/ILightClient.sol";
import { ILightClientMsgs } from "../src/msgs/ILightClientMsgs.sol";
import { DummyLightClient } from "./DummyLightClient.sol";

contract ICS02ClientTest is Test {
    IICS02Client public ics02Client;
    DummyLightClient public lightClient;

    function setUp() public {
        lightClient = new DummyLightClient(ILightClientMsgs.UpdateResult.Update, 0);
        ics02Client = new ICS02Client(address(this));
    }

    function test_ICS02Client() public {
        string memory counterpartyClient = "42-dummy-01";
        string memory clientIdentifier = ics02Client.addClient(
            "07-tendermint", IICS02ClientMsgs.CounterpartyInfo(counterpartyClient), address(lightClient)
        );

        ILightClient fetchedLightClient = ics02Client.getClient(clientIdentifier);
        assertNotEq(address(fetchedLightClient), address(0), "client not found");

        IICS02Client.CounterpartyInfo memory counterpartyInfo = ics02Client.getCounterparty(clientIdentifier);
        assertEq(counterpartyInfo.clientId, counterpartyClient, "counterpartyInfo not found");

        bytes memory updateMsg = "testUpdateMsg";
        ILightClient.UpdateResult updateResult = ics02Client.updateClient(clientIdentifier, updateMsg);
        assertEq(uint256(updateResult), uint256(ILightClientMsgs.UpdateResult.Update), "updateClient failed");
        assertEq(updateMsg, lightClient.latestUpdateMsg(), "updateClient failed");
    }
}