// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

/*
    This script is used for deploying the demo contracts on testnets
*/

// solhint-disable custom-errors,gas-custom-errors

import { stdJson } from "forge-std/StdJson.sol";
import { Script } from "forge-std/Script.sol";

import { ICS26Router } from "../contracts/ICS26Router.sol";
import { ICS27GMP } from "../contracts/ICS27GMP.sol";
import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";
import { ICS27Lib } from "../contracts/utils/ICS27Lib.sol";
import { ERC1967Proxy } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Proxy.sol";
import { DeployAccessManagerWithRoles } from "./deployments/DeployAccessManagerWithRoles.sol";
import { ICS27Account } from "../contracts/utils/ICS27Account.sol";
import { AccessManager } from "@openzeppelin-contracts/access/manager/AccessManager.sol";
import { IBCXERC20 } from "../contracts/demo/IBCXERC20.sol";

contract DemoDeploy is Script, DeployAccessManagerWithRoles {
    using stdJson for string;

    string internal constant DEPLOYMENTS_DIR = "./deployments/";

    // solhint-disable-next-line function-max-lines
    function run() public returns (string memory) {
        vm.startBroadcast();

        // Deploy IBC Eureka with proxy
        address ics26RouterLogic = address(new ICS26Router());
        address ics27GmpLogic = address(new ICS27GMP());
        address ibcxerc20Logic = address(new IBCXERC20());

        AccessManager accessManager = new AccessManager(msg.sender);

        ERC1967Proxy routerProxy =
            new ERC1967Proxy(ics26RouterLogic, abi.encodeCall(ICS26Router.initialize, (address(accessManager))));

        ERC1967Proxy gmpProxy = new ERC1967Proxy(
            ics27GmpLogic,
            abi.encodeCall(
                ICS27GMP.initialize, (address(routerProxy), address(new ICS27Account()), address(accessManager))
            )
        );

        // Wire up the IBCAdmin and access control
        accessManagerSetTargetRoles(accessManager, address(routerProxy), makeAddr("ics20"), true);

        accessManagerSetRoles(
            accessManager, new address[](0), new address[](0), new address[](0), msg.sender, msg.sender, msg.sender
        );

        // Wire GMP app
        ICS26Router(address(routerProxy)).addIBCApp(ICS27Lib.DEFAULT_PORT_ID, address(gmpProxy));

        // IBCXERC20
        IBCXERC20 ibcxerc20 = IBCXERC20(
            address(
                new ERC1967Proxy(
                    ibcxerc20Logic,
                    abi.encodeCall(IBCXERC20.initialize, (msg.sender, "WildFlower", "WF", address(gmpProxy)))
                )
            )
        );
        ibcxerc20.mint(msg.sender, 10_000);

        vm.stopBroadcast();

        string memory json = "json";
        json.serialize("ics26Router", Strings.toHexString(address(routerProxy)));
        json.serialize("ics27Gmp", Strings.toHexString(address(gmpProxy)));
        string memory finalJson = json.serialize("ibcxerc20", Strings.toHexString(address(ibcxerc20)));

        string memory fileName = string.concat(DEPLOYMENTS_DIR, "testnet.json");
        vm.writeFile(fileName, finalJson);

        return finalJson;
    }
}
