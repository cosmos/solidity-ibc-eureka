// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

import { Deployments } from "../helpers/Deployments.sol";
import { stdJson } from "forge-std/StdJson.sol";
import { ICS26Router } from "../../contracts/ICS26Router.sol";
import { ERC1967Proxy } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Proxy.sol";

library DeployProxiedICS26Router {
    using stdJson for string;

    function deploy(Deployments.ProxiedICS26RouterDeployment memory deployment) public returns (ERC1967Proxy) {
        ERC1967Proxy routerProxy = new ERC1967Proxy(
            deployment.implementation,
            abi.encodeWithSelector(ICS26Router.initialize.selector, deployment.timeLockAdmin, deployment.timeLockAdmin)
        );

        return routerProxy;
    }
}
