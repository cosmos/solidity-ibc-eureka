// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

/*
    This script is used for deploying to a live network (it could be local, but is geared towards testnet or mainnet)
*/

import { Script } from "forge-std/Script.sol";
import { DeployLib } from "./DeployLib.sol";
import { IICS26Router } from "../contracts/interfaces/IICS26Router.sol";
import { IICS20Transfer } from "../contracts/interfaces/IICS20Transfer.sol";
import "forge-std/console.sol";
import { Escrow } from "../contracts/utils/Escrow.sol";
import { ICS26Router } from "../contracts/ICS26Router.sol";
import { IICS26Router } from "../contracts/interfaces/IICS26Router.sol";
import { IICS20Transfer } from "../contracts/interfaces/IICS20Transfer.sol";
import { ICS20Transfer } from "../contracts/ICS20Transfer.sol";
import { ICS20Lib } from "../contracts/utils/ICS20Lib.sol";
import { ERC1967Proxy } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Proxy.sol";
import { IBCERC20 } from "../contracts/utils/IBCERC20.sol";

/// @dev See the Solidity Scripting tutorial: https://book.getfoundry.sh/guides/scripting-with-solidity
contract QueryIBC is Script {
    function run() public {
        address routerAddress = vm.promptAddress("Enter ICS26 Router address");

        IICS26Router router = IICS26Router(routerAddress);

    }
}
