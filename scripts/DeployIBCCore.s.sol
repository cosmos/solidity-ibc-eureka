// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

/*
    This script is used for deploying to a live network (it could be local, but is geared towards testnet or mainnet)
*/

import { Script } from "forge-std/Script.sol";
import { DeployLib } from "./DeployLib.sol";
import { Escrow } from "../contracts/utils/Escrow.sol";
import { ICS26Router } from "../contracts/ICS26Router.sol";
import { ICS20Transfer } from "../contracts/ICS20Transfer.sol";
import { ICS20Lib } from "../contracts/utils/ICS20Lib.sol";
import { ERC1967Proxy } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Proxy.sol";
import { IBCERC20 } from "../contracts/utils/IBCERC20.sol";
import "forge-std/console.sol";

/// @dev See the Solidity Scripting tutorial: https://book.getfoundry.sh/guides/scripting-with-solidity
contract DeployIBCCore is Script {
    string internal constant ENV_DEPLOYMENT_CONFIG_FILEPATH = "DEPLOYMENT_CONFIG_FILEPATH";

    function run() public {
        // ============ Step 1: Load parameters ==============
        string memory configFilePath = vm.envString(ENV_DEPLOYMENT_CONFIG_FILEPATH);
        string memory deploymentConfigJson = vm.readFile(configFilePath);
        DeployLib.DeploymentConfigJson memory deploymentConfig = DeployLib.loadDeploymentConfigFromJson(deploymentConfigJson);

        // ============ Step 2: Deploy the contracts ==============
        vm.startBroadcast();

        // Deploy IBC Eureka
        // Deploy IBC Eureka with proxy
        address escrowLogic = address(new Escrow());
        address ibcERC20Logic = address(new IBCERC20());
        address ics26RouterLogic = address(new ICS26Router());
        address ics20TransferLogic = address(new ICS20Transfer());

        ERC1967Proxy routerProxy = new ERC1967Proxy(
            ics26RouterLogic,
            abi.encodeWithSelector(
                ICS26Router.initialize.selector,
                deploymentConfig.timelockAdminAddress,
                deploymentConfig.portCustomizerAddress
            )
        );

        ERC1967Proxy transferProxy = new ERC1967Proxy(
            ics20TransferLogic,
            abi.encodeWithSelector(
                ICS20Transfer.initialize.selector,
                address(routerProxy),
                escrowLogic,
                ibcERC20Logic,
                deploymentConfig.ics20PauserAddress,
                deploymentConfig.permit2Address
            )
        );

        ICS26Router ics26Router = ICS26Router(address(routerProxy));
        ICS20Transfer ics20Transfer = ICS20Transfer(address(transferProxy));

        ics26Router.addIBCApp(ICS20Lib.DEFAULT_PORT_ID, address(ics20Transfer));

        vm.stopBroadcast();

        console.log("ICS26Router address", address(ics26Router));
        console.log("ICS20Transfer address", address(ics20Transfer));
    }
}
