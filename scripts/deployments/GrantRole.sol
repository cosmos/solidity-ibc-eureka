// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

// solhint-disable custom-errors,gas-custom-errors

import { Script } from "forge-std/Script.sol";
import { Deployments } from "../helpers/Deployments.sol";
import { ERC1967Proxy } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Proxy.sol";
import { IBeacon } from "@openzeppelin-contracts/proxy/beacon/IBeacon.sol";
import { ERC1967Utils } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Utils.sol";
import { ICS20Transfer } from "../../contracts/ICS20Transfer.sol";
import { IICS26Router } from "../../contracts/interfaces/IICS26Router.sol";
import { ICS20Lib } from "../../contracts/utils/ICS20Lib.sol";
import { stdJson } from "forge-std/StdJson.sol";
import { IBCERC20 } from "../../contracts/utils/IBCERC20.sol";
import { Escrow } from "../../contracts/utils/Escrow.sol";
import { IBCPausableUpgradeable } from "../../contracts/utils/IBCPausableUpgradeable.sol";
import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";

contract GrantRoleScript is Deployments, Script {

    // function verify(ProxiedICS20TransferDeployment memory deployment) internal view {
    // }

    function run() public returns (address){
        string memory root = vm.projectRoot();
        string memory deployEnv = vm.envString("DEPLOYMENT_ENV");
        string memory path = string.concat(root, DEPLOYMENT_DIR, "/", deployEnv, "/", Strings.toString(block.chainid), ".json");
        string memory json = vm.readFile(path);

        bool verifyOnly = vm.envOr("VERIFY_ONLY", false);

        ProxiedICS20TransferDeployment memory deployment = loadProxiedICS20TransferDeployment(vm, json);

        // TODO: Check the address
        // if ((deployment.implementation != address(0) || deployment.proxy != address(0)) || verifyOnly) {
        //     verify(deployment);
        //     return deployment.proxy;
        // }

        if (deployment.ics26Router == address(0)) {
            revert("ICS26Router not set");
        }

        vm.startBroadcast();

        if (deployment.implementation == address(0)) {
            revert("Implementation not set");
        }

        if (deployment.ibcERC20Implementation == address(0)) {
            revert("IBCERC20Implementation not set");
        }

        if (deployment.escrowImplementation == address(0)) {
            revert("EscrowImplementation not set");
        }

        // TODO: Do properly: 
        address account = 0x92470162374A6D185758982356833d1aFfFd3b03;
        ICS20Transfer ics20Transfer = ICS20Transfer(deployment.proxy);
        ics20Transfer.grantDelegateSenderRole(account);


        vm.stopBroadcast();

        // TODO: Do the other stuff
        return address(account);
    }
}
