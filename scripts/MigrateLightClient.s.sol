// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

import { Script } from "forge-std/Script.sol";
import { IICS02Client } from "../contracts//interfaces/IICS02Client.sol";
import { IICS02ClientMsgs } from "../contracts/msgs/IICS02ClientMsgs.sol";

contract MigrateLightClient is Script {
    function run() public {
        address ics26RouterAddress = vm.promptAddress("ICS26 Router proxy address");
        string memory clientIDToMigrate = vm.prompt("Client ID to migrate");
        address newLightClientAddress = vm.promptAddress("New light client address");

        IICS02Client ics26Router = IICS02Client(ics26RouterAddress);

        address actualClientAddress = address(ics26Router.getClient(clientIDToMigrate));

        vm.assertNotEq(actualClientAddress, newLightClientAddress, "Clients must not match");

        IICS02ClientMsgs.CounterpartyInfo memory counterPartyInfo = ics26Router.getCounterparty(clientIDToMigrate);

        vm.startBroadcast();

        ics26Router.migrateClient(clientIDToMigrate, counterPartyInfo, newLightClientAddress);

        vm.stopBroadcast();
    }
}
