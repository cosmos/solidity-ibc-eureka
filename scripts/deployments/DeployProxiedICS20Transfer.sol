// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

// solhint-disable custom-errors,gas-custom-errors

// solhint-disable-next-line no-global-import
import "forge-std/console.sol";

import { Script } from "forge-std/Script.sol";
import { Deployments } from "../helpers/Deployments.sol";
import { ERC1967Proxy } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Proxy.sol";
import { IBeacon } from "@openzeppelin-contracts/proxy/beacon/IBeacon.sol";
import { ERC1967Utils } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Utils.sol";
import { ICS20Transfer } from "../../contracts/ICS20Transfer.sol";
import { IICS26Router } from "../../contracts/interfaces/IICS26Router.sol";
import { ICS20Lib } from "../../contracts/utils/ICS20Lib.sol";
import { IBCERC20 } from "../../contracts/utils/IBCERC20.sol";
import { Escrow } from "../../contracts/utils/Escrow.sol";
import { IBCPausableUpgradeable } from "../../contracts/utils/IBCPausableUpgradeable.sol";
import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";


abstract contract DeployProxiedICS20Transfer is Deployments {
    function deployProxiedICS20Transfer(ProxiedICS20TransferDeployment memory deployment) public returns (ERC1967Proxy) {
        ERC1967Proxy transferProxy = new ERC1967Proxy(
            deployment.implementation,
            abi.encodeCall(
                ICS20Transfer.initialize,
                (
                    deployment.ics26Router,
                    deployment.escrowImplementation,
                    deployment.ibcERC20Implementation,
                    deployment.permit2
                )
            )
        );

        console.log("Deployed ICS20Transfer at address: ", address(transferProxy));

        ICS20Transfer ics20Transfer = ICS20Transfer(address(transferProxy));

        if (deployment.pausers.length != 0) {
            for (uint32 i = 0; i < deployment.pausers.length; i++) {
                address pauser = deployment.pausers[i];
                console.log("Granting pauser role to: ", pauser);
                ics20Transfer.grantPauserRole(pauser);
            }
        }

        if (deployment.unpausers.length != 0) {
            for (uint32 i = 0; i < deployment.unpausers.length; i++) {
                address unpauser = deployment.unpausers[i];
                console.log("Granting unpauser role to: ", unpauser);
                ics20Transfer.grantUnpauserRole(unpauser);
            }
        }

        if (deployment.tokenOperator != address(0)) {
            address tokenOperator = deployment.tokenOperator;
            console.log("Granting tokenOperator role to: ", tokenOperator);
            ics20Transfer.grantTokenOperatorRole(tokenOperator);
        }

        return transferProxy;
    }
}
