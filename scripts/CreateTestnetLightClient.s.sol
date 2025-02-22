// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

/*
    This script is used for deploying the SP1ICS07Tendermint light client to a live network (it could be local, but is geared towards testnet)
*/

import { Script } from "forge-std/Script.sol";
import { IICS02Client } from "../contracts/interfaces/IICS02Client.sol";
import { IICS02ClientMsgs } from "../contracts/msgs/IICS02ClientMsgs.sol";
import { ISP1ICS07Tendermint } from "../contracts/light-clients/SP1ICS07Tendermint.sol";
import { DeployLib } from "./DeployLib.sol";
import "forge-std/console.sol";

/// @dev See the Solidity Scripting tutorial: https://book.getfoundry.sh/guides/scripting-with-solidity
contract CreateTestnetLightClient is Script {
    bytes[] public merklePrefix = [bytes("ibc"), bytes("")];

    function run() public {
        address ics26router = vm.promptAddress("Enter the ics26 router address");
        address verifier = vm.promptAddress("Enter the verifier address");
        string memory counterpartyClientID = vm.prompt("Enter the counterparty client ID");

        string memory root = vm.projectRoot();
        string memory tendermintGenesisJson = vm.readFile(string.concat(root, "/scripts/genesis.json"));
        DeployLib.SP1ICS07TendermintGenesisJson memory genesis = DeployLib.loadTendermintGenesisFromJson(tendermintGenesisJson);


        IICS02ClientMsgs.CounterpartyInfo memory counterpartyInfo = IICS02ClientMsgs.CounterpartyInfo(
            counterpartyClientID,
            merklePrefix
        );

        IICS02Client router = IICS02Client(ics26router);

        vm.startBroadcast();

        // Deploy new light client
        ISP1ICS07Tendermint ics07Tendermint = DeployLib.deployTendermintLightClient(genesis, verifier);
        string memory clientID = router.addClient(counterpartyInfo, address(ics07Tendermint));

        vm.stopBroadcast();

        console.log("Light client added with client ID:", clientID);
    }
}
