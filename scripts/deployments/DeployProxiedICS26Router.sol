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
    function deployProxiedICS26Router(Deployments.ProxiedICS26RouterDeployment memory deployment) public returns (ERC1967Proxy) {
        require(msg.sender == deployment.timeLockAdmin, "sender must be timeLockAdmin");

        ERC1967Proxy routerProxy = new ERC1967Proxy(
            deployment.implementation,
            abi.encodeCall(ICS26Router.initialize, (deployment.timeLockAdmin))
        );

        ICS26Router ics26Router = ICS26Router(address(routerProxy));

        for (uint256 i = 0; i < deployment.relayers.length; i++) {
            ics26Router.grantRole(ics26Router.RELAYER_ROLE(), deployment.relayers[i]);
        }
         ics26Router.grantRole(ics26Router.PORT_CUSTOMIZER_ROLE(), deployment.portCustomizer);
         ics26Router.grantRole(ics26Router.CLIENT_ID_CUSTOMIZER_ROLE(), deployment.clientIdCustomizer);

        return routerProxy;
    }
}
