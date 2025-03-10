// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

import { Deployments } from "../helpers/Deployments.sol";
import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";
import { Script } from "forge-std/Script.sol";
import { ICS20Transfer } from "../../contracts/ICS20Transfer.sol";

contract PauseTransfers is Script, Deployments {
    function run() public {
        string memory root = vm.projectRoot();
        string memory deployEnv = vm.envString("DEPLOYMENT_ENV");
        string memory path = string.concat(root, DEPLOYMENT_DIR, "/", deployEnv, "/", Strings.toString(block.chainid), ".json");
        string memory json = vm.readFile(path);

        ProxiedICS20TransferDeployment memory deployment = loadProxiedICS20TransferDeployment(vm, json);

        ICS20Transfer ics20Transfer = ICS20Transfer(deployment.proxy);

        vm.broadcast();
        ics20Transfer.pause();

        vm.assertTrue(ics20Transfer.paused(), "ICS20Transfer should be paused");
    }
}
