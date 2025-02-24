// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

import "../../contracts/ICS26Router.sol";
import {Script} from "forge-std/Script.sol";
import {stdJson} from "forge-std/StdJson.sol";

contract ICS26RouterDeployScript is Script {
    using stdJson for string;

    function deploy() public returns (address) {
        ICS26Router ics26Router = new ICS26Router();

        return address(ics26Router);
    }

    function run() public returns (address) {
        return deploy();
    }
}