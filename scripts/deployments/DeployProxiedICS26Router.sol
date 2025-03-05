// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

// solhint-disable custom-errors,gas-custom-errors

import { Deployments } from "../helpers/Deployments.sol";
import { stdJson } from "forge-std/StdJson.sol";
import { ICS26Router } from "../../contracts/ICS26Router.sol";
import { IIBCUUPSUpgradeable } from "../../contracts/interfaces/IIBCUUPSUpgradeable.sol";
import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";
import { ERC1967Proxy } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Proxy.sol";
import { ERC1967Utils } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Utils.sol";
import { Script } from "forge-std/Script.sol";

abstract contract DeployProxiedICS26Router is Deployments {
    using stdJson for string;

    function deployProxiedICS26Router(Deployments.ProxiedICS26RouterDeployment memory deployment) public returns (ERC1967Proxy) {
        ERC1967Proxy routerProxy = new ERC1967Proxy(
            deployment.implementation,
            abi.encodeWithSelector(ICS26Router.initialize.selector, deployment.timeLockAdmin, deployment.timeLockAdmin)
        );

        return routerProxy;
    }
}

contract DeployProxiedICS26RouterScript is Script, DeployProxiedICS26Router {
    using stdJson for string;

    function getImplementation(address proxy) internal view returns (address) {
        return address(uint160(uint256(vm.load(proxy, ERC1967Utils.IMPLEMENTATION_SLOT))));
    }

    function verify(ProxiedICS26RouterDeployment memory deployment) internal view {
        ERC1967Proxy routerProxy = ERC1967Proxy(deployment.proxy);

        vm.assertEq(
            getImplementation(address(routerProxy)),
            deployment.implementation,
            "implementation addresses don't match"
        );

        IIBCUUPSUpgradeable uups = IIBCUUPSUpgradeable(address(routerProxy));

        vm.assertEq(
            uups.getTimelockedAdmin(),
            deployment.timeLockAdmin,
            "timelockAdmin addresses don't match"
        );
    }

    function run() public returns (address){
        string memory root = vm.projectRoot();
        string memory deployEnv = vm.envString("DEPLOYMENT_ENV");
        string memory path = string.concat(root, DEPLOYMENT_DIR, "/", deployEnv, "/", Strings.toString(block.chainid), ".json");
        string memory json = vm.readFile(path);

        bool verifyOnly = vm.envOr("VERIFY_ONLY", false);

        ProxiedICS26RouterDeployment memory deployment = loadProxiedICS26RouterDeployment(vm, json);

        if ((deployment.implementation != address(0) || deployment.proxy != address(0)) || verifyOnly) {
            verify(deployment);
            return deployment.proxy;
        }

        vm.startBroadcast();

        if (deployment.implementation == address(0)) {
            deployment.implementation = address(new ICS26Router());
        }

        ERC1967Proxy routerProxy = deployProxiedICS26Router(deployment);
        deployment.proxy = payable(address(routerProxy));

        vm.stopBroadcast();

        verify(deployment);

        vm.serializeAddress("ics26Router", "proxy", address(routerProxy));
        vm.serializeAddress("ics26Router", "implementation", deployment.implementation);
        string memory output = vm.serializeAddress("ics26Router", "timeLockAdmin", deployment.timeLockAdmin);

        vm.writeJson(output, path, ".ics26Router");
        vm.writeJson(vm.toString(address(routerProxy)), path, ".ics20Transfer.ics26Router");

        return address(routerProxy);
    }
}

