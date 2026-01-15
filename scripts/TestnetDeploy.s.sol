// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

/*
    This script deploys the core IBC contracts for testnets.
*/

// solhint-disable custom-errors,gas-custom-errors

import { stdJson } from "forge-std/StdJson.sol";
import { Script } from "forge-std/Script.sol";

import { ICS26Router } from "../contracts/ICS26Router.sol";
import { ICS20Transfer } from "../contracts/ICS20Transfer.sol";
import { ICS27GMP } from "../contracts/ICS27GMP.sol";
import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";
import { ICS20Lib } from "../contracts/utils/ICS20Lib.sol";
import { ICS27Lib } from "../contracts/utils/ICS27Lib.sol";
import { ERC1967Proxy } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Proxy.sol";
import { DeployAccessManagerWithRoles } from "./deployments/DeployAccessManagerWithRoles.sol";
import { IBCERC20 } from "../contracts/utils/IBCERC20.sol";
import { Escrow } from "../contracts/utils/Escrow.sol";
import { ICS27Account } from "../contracts/utils/ICS27Account.sol";
import { IFTAccessManaged } from "../contracts/utils/IFTAccessManaged.sol";
import { AccessManager } from "@openzeppelin-contracts/access/manager/AccessManager.sol";

/// @title Testnet Deployment Script
/// @dev See the Solidity Scripting tutorial: https://book.getfoundry.sh/tutorials/solidity-scripting
contract TestnetDeploy is Script, DeployAccessManagerWithRoles {
    using stdJson for string;

    // Permit2 contract address on Ethereum mainnet and testnets
    address internal constant PERMIT2 = 0x000000000022D473030F116dDEE9F6B43aC78BA3;
    string internal constant IFT_TOKEN_NAME = "IFT Fungible Token";
    string internal constant IFT_TOKEN_SYMBOL = "IFT";

    function run() public returns (string memory) {
        // ============ Step 1: Deploy the contracts ==============
        vm.startBroadcast();

        // Deploy IBC Eureka with proxy
        address ics26RouterLogic = address(new ICS26Router());
        address ics20TransferLogic = address(new ICS20Transfer());
        address ics27GmpLogic = address(new ICS27GMP());

        AccessManager accessManager = new AccessManager(msg.sender);

        ERC1967Proxy routerProxy =
            new ERC1967Proxy(ics26RouterLogic, abi.encodeCall(ICS26Router.initialize, (address(accessManager))));

        ERC1967Proxy transferProxy = new ERC1967Proxy(
            ics20TransferLogic,
            abi.encodeCall(
                ICS20Transfer.initialize,
                (address(routerProxy), address(new Escrow()), address(new IBCERC20()), PERMIT2, address(accessManager))
            )
        );

        ERC1967Proxy gmpProxy = new ERC1967Proxy(
            ics27GmpLogic,
            abi.encodeCall(
                ICS27GMP.initialize, (address(routerProxy), address(new ICS27Account()), address(accessManager))
            )
        );

        IFTAccessManaged iftImpl = new IFTAccessManaged();
        ERC1967Proxy iftProxy = new ERC1967Proxy(
            address(iftImpl),
            abi.encodeCall(
                IFTAccessManaged.initialize,
                (address(accessManager), IFT_TOKEN_NAME, IFT_TOKEN_SYMBOL, address(gmpProxy))
            )
        );

        // Wire up the IBCAdmin and access control
        accessManagerSetTargetRoles(accessManager, address(routerProxy), address(transferProxy), true);
        accessManagerSetRoles(
            accessManager, new address[](0), new address[](0), new address[](0), msg.sender, msg.sender, msg.sender
        );

        // Wire Transfer + GMP apps
        ICS26Router(address(routerProxy)).addIBCApp(ICS20Lib.DEFAULT_PORT_ID, address(transferProxy));
        ICS26Router(address(routerProxy)).addIBCApp(ICS27Lib.DEFAULT_PORT_ID, address(gmpProxy));

        vm.stopBroadcast();

        string memory json = "json";
        json.serialize("accessManager", Strings.toHexString(address(accessManager)));
        json.serialize("ics26Router", Strings.toHexString(address(routerProxy)));
        json.serialize("ics20Transfer", Strings.toHexString(address(transferProxy)));
        json.serialize("ics27Gmp", Strings.toHexString(address(gmpProxy)));
        string memory finalJson = json.serialize("iftToken", Strings.toHexString(address(iftProxy)));

        return finalJson;
    }
}
