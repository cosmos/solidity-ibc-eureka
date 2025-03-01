// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

/*
    This script is used for deploying the SP1ICS07Tendermint light client to a live network (it could be local, but is geared towards testnet)
*/

import { Script } from "forge-std/Script.sol";
import { IICS02Client } from "../contracts/interfaces/IICS02Client.sol";
import { IICS02ClientMsgs } from "../contracts/msgs/IICS02ClientMsgs.sol";
import { IICS07TendermintMsgs } from "../contracts/light-clients/msgs/IICS07TendermintMsgs.sol";
import { SP1ICS07Tendermint } from "../contracts/light-clients/SP1ICS07Tendermint.sol";
import { TendermintLib } from "./utils/TendermintLib.sol";
import "forge-std/console.sol";

/// @dev See the Solidity Scripting tutorial: https://book.getfoundry.sh/guides/scripting-with-solidity
contract CreateTestnetLightClient is Script {
    bytes[] public merklePrefix = [bytes("ibc"), bytes("")];

    function run() public {
        address ics26router = vm.promptAddress("Enter the ics26 router address");
        address sp1Verifier = vm.promptAddress("Enter the verifier address");
        string memory counterpartyClientID = vm.prompt("Enter the counterparty client ID");

        string memory root = vm.projectRoot();
        string memory tendermintGenesisJson = vm.readFile(string.concat(root, "/scripts/genesis.json"));
        TendermintLib.SP1ICS07TendermintGenesisJson memory genesis = TendermintLib.loadTendermintGenesisFromJson(tendermintGenesisJson);

        IICS02ClientMsgs.CounterpartyInfo memory counterpartyInfo = IICS02ClientMsgs.CounterpartyInfo(
            counterpartyClientID,
            merklePrefix
        );

        IICS02Client router = IICS02Client(ics26router);

        vm.startBroadcast();

        // Deploy new light client
        IICS07TendermintMsgs.ConsensusState memory trustedConsensusState =
            abi.decode(genesis.trustedConsensusState, (IICS07TendermintMsgs.ConsensusState));

        // Deploy the SP1 ICS07 Tendermint light client
        SP1ICS07Tendermint ics07Tendermint = new SP1ICS07Tendermint(
            genesis.updateClientVkey,
            genesis.membershipVkey,
            genesis.ucAndMembershipVkey,
            genesis.misbehaviourVkey,
            sp1Verifier,
            genesis.trustedClientState,
            keccak256(abi.encode(trustedConsensusState))
        );
        string memory clientID = router.addClient(counterpartyInfo, address(ics07Tendermint));

        vm.stopBroadcast();

        console.log("Light client added with client ID:", clientID);
    }
}
