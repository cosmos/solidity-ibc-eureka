
// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";
import { Script } from "forge-std/Script.sol";
import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";
import { IICS02Client } from "../contracts//interfaces/IICS02Client.sol";
import { IICS02ClientMsgs } from "../contracts/msgs/IICS02ClientMsgs.sol";

contract MigrateLightClient is Script {
    function run() public {
        address ics26RouterAddress = vm.promptAddress("ICS26 Router proxy address");
        string memory clientIDToMigrate = vm.prompt("Client ID to migrate");
        address newLightClientAddress = vm.promptAddress("New light client address");

        IICS02Client ics26Router = IICS02Client(ics26RouterAddress);

        address actualClientAddress = address(ics26Router.getClient(clientIDToMigrate));

        vm.assertNotEq(actualClientAddress, newLightClientAddress, "On-chain client address already matches the implementation address");

        IICS02ClientMsgs.CounterpartyInfo memory counterPartyInfo = ics26Router.getCounterparty(clientIDToMigrate);

        vm.startBroadcast();

        ics26Router.migrateClient(clientIDToMigrate, counterPartyInfo, newLightClientAddress);

        vm.stopBroadcast();
    }
}
