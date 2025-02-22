// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

import { Script } from "forge-std/Script.sol";
import { IICS02ClientMsgs } from "../contracts/msgs/IICS02ClientMsgs.sol";
import { ICS02ClientUpgradeable } from "../contracts/utils/ICS02ClientUpgradeable.sol";
import { TendermintLib } from "./utils/TendermintLib.sol";
import { IICS07TendermintMsgs } from "../contracts/light-clients/msgs/IICS07TendermintMsgs.sol";
import { SP1ICS07Tendermint } from "../contracts/light-clients/SP1ICS07Tendermint.sol";
import "forge-std/console.sol";

/// @dev See the Solidity Scripting tutorial: https://book.getfoundry.sh/guides/scripting-with-solidity
contract MigrateLightClient is Script {
    function run() public {
        address ics26Address = vm.promptAddress("Enter the ics26 router address");
        address verifier = vm.promptAddress("Enter the SP1 verifier address");
        string memory subjectClientID = vm.prompt("Enter existing client ID (e.g. 'client-0')");

        ICS02ClientUpgradeable ics26 = ICS02ClientUpgradeable(ics26Address);
        IICS02ClientMsgs.CounterpartyInfo memory counterpartyInfo = ics26.getCounterparty(subjectClientID);

        string memory root = vm.projectRoot();
        string memory tendermintGenesisJson = vm.readFile(string.concat(root, "/scripts/genesis.json"));
        TendermintLib.SP1ICS07TendermintGenesisJson memory genesis = TendermintLib.loadTendermintGenesisFromJson(tendermintGenesisJson);

        vm.startBroadcast();

        // Deploy new light client
        IICS07TendermintMsgs.ConsensusState memory trustedConsensusState =
            abi.decode(genesis.trustedConsensusState, (IICS07TendermintMsgs.ConsensusState));

        // Deploy the SP1 ICS07 Tendermint light client
        SP1ICS07Tendermint ics07Tendermint =  new SP1ICS07Tendermint(
            genesis.updateClientVkey,
            genesis.membershipVkey,
            genesis.ucAndMembershipVkey,
            genesis.misbehaviourVkey,
            verifier,
            genesis.trustedClientState,
            keccak256(abi.encode(trustedConsensusState))
        );

        // Add client with same counterparty info as the existing client
        string memory substituteClientID = ics26.addClient(counterpartyInfo, address(ics07Tendermint));
        // Migrate existing client to the new client
        ics26.migrateClient(subjectClientID, substituteClientID);

        vm.stopBroadcast();

        console.log("Deployed new ICS07 Tendermint Light Client", address(ics07Tendermint));
        console.log("Upgraded Ligth Client");
    }
}

