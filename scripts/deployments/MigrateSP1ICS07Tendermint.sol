
// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

import { Deployments } from "../helpers/Deployments.sol";
import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";
import { Script } from "forge-std/Script.sol";
import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";
import { SP1ICS07Tendermint } from "../../contracts/light-clients/SP1ICS07Tendermint.sol";
import { IICS07TendermintMsgs } from "../../contracts/light-clients/msgs/IICS07TendermintMsgs.sol";
import { IICS02Client } from "../../contracts/interfaces/IICS02Client.sol";
import { IICS02ClientMsgs } from "../../contracts/msgs/IICS02ClientMsgs.sol";

contract MigrateSP1ICS07Tendermint is Script, Deployments {
    function run() public {
        string memory root = vm.projectRoot();
        string memory deployEnv = vm.envString("DEPLOYMENT_ENV");
        string memory path = string.concat(root, DEPLOYMENT_DIR, "/", deployEnv, "/", Strings.toString(block.chainid), ".json");
        string memory deploymentJson = vm.readFile(path);

        string memory clientIDToMigrate = vm.prompt("Client ID to migrates");

        SP1ICS07TendermintDeployment[] memory deployments = loadSP1ICS07TendermintDeployments(vm, deploymentJson);
        ProxiedICS26RouterDeployment memory ics26RouterDeployment = loadProxiedICS26RouterDeployment(vm, deploymentJson);

        uint256 deploymentIndex = UINT256_MAX;
        for (uint256 i = 0; i < deployments.length; i++) {
            if (Strings.equal(deployments[i].clientId, clientIDToMigrate)) {
                deploymentIndex = uint256(i);
                break;
            }
        }
        vm.assertNotEq(deploymentIndex, UINT256_MAX, "Client ID not found");
        SP1ICS07TendermintDeployment memory deploymentToMigrate = deployments[deploymentIndex];

        vm.startBroadcast();

        // Deploy the replacement contract
        IICS07TendermintMsgs.ConsensusState memory trustedConsensusState =
            abi.decode(deploymentToMigrate.trustedConsensusState, (IICS07TendermintMsgs.ConsensusState));
        (bool success, address verifier) = Strings.tryParseAddress(deploymentToMigrate.verifier);
        vm.assertTrue(success, string.concat("Invalid verifier address: ", deploymentToMigrate.verifier));

        SP1ICS07Tendermint replacementLightClient = new SP1ICS07Tendermint(
            deploymentToMigrate.updateClientVkey,
            deploymentToMigrate.membershipVkey,
            deploymentToMigrate.ucAndMembershipVkey,
            deploymentToMigrate.misbehaviourVkey,
            verifier,
            deploymentToMigrate.trustedClientState,
            keccak256(abi.encode(trustedConsensusState)),
            deploymentToMigrate.proofSubmitter
        );

        IICS02Client ics26Router = IICS02Client(ics26RouterDeployment.proxy);
        bytes[] memory merklePrefix = new bytes[](deploymentToMigrate.merklePrefix.length);
        for (uint256 i = 0; i < deploymentToMigrate.merklePrefix.length; i++) {
            merklePrefix[i] = bytes(deploymentToMigrate.merklePrefix[i]);
        }
        IICS02ClientMsgs.CounterpartyInfo memory counterpartyInfo = IICS02ClientMsgs.CounterpartyInfo(deploymentToMigrate.counterpartyClientId, merklePrefix);
        string memory substituteClientID = ics26Router.addClient(counterpartyInfo, address(replacementLightClient));

        // TODO: Make this an output that can be used as a multisig prop
        ics26Router.migrateClient(clientIDToMigrate, substituteClientID);

        vm.stopBroadcast();

        // Update the deployment JSON
        vm.writeJson(vm.toString(address(replacementLightClient)), path, string.concat(".light_clients['", Strings.toString(deploymentIndex), "'].implementation"));
    }
}
