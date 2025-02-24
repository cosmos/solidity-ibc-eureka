// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

import {Deployments} from "../helpers/Deployments.sol";
import {stdJson} from "forge-std/StdJson.sol";
import {TimelockController} from "@openzeppelin-contracts/governance/TimelockController.sol";
import { ICS20Transfer } from "../../contracts/ICS20Transfer.sol";
import { ERC1967Proxy } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Proxy.sol";

library DeployProxiedICS20Transfer {
    using stdJson for string;

    function deploy(Deployments.ProxiedICS20TransferDeployment memory deployment) public returns (ERC1967Proxy){
        ERC1967Proxy transferProxy = new ERC1967Proxy(
            deployment.implementation,
            abi.encodeWithSelector(
                ICS20Transfer.initialize.selector,
                deployment.ics26Router,
                deployment.escrow,
                deployment.ibcERC20,
                deployment.pauserAddress,
                deployment.permit2Address
            )
        );

        return transferProxy;
    }
}
