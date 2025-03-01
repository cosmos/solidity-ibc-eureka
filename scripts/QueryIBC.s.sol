// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

/*
    This script is used for deploying to a live network (it could be local, but is geared towards testnet or mainnet)
*/

import { Script } from "forge-std/Script.sol";
import "forge-std/console.sol";
import { IICS02Client } from "../contracts/interfaces/IICS02Client.sol";
import { IICS02ClientMsgs } from "../contracts/msgs/IICS02ClientMsgs.sol";
import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";

/// @dev See the Solidity Scripting tutorial: https://book.getfoundry.sh/guides/scripting-with-solidity
contract QueryIBC is Script {
    function run() public {
        address routerAddress = vm.promptAddress("Enter ICS26 Router address");

        IICS02Client client = IICS02Client(routerAddress);

        uint256 seq = client.getNextClientSeq();

        console.log("Number of clients:", seq);

        for (uint256 i = 0; i < seq; i++) {
            string memory clientID = string.concat("client-", Strings.toString(i));
            console.log(clientID);
            IICS02ClientMsgs.CounterpartyInfo memory counterparty = client.getCounterparty(clientID);
            console.log("Counterparty client ID:", counterparty.clientId);
        }
    }
}
