// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

import "../../contracts/utils/IBCERC20.sol";
import {Script} from "forge-std/Script.sol";
import {stdJson} from "forge-std/StdJson.sol";

contract IBCERC20Deploy is Script {
    using stdJson for string;

    function deploy() public returns (address) {
        IBCERC20 ibcERC20 = new IBCERC20();

        return address(ibcERC20);
    }

    function run() public returns (address) {
        return deploy();
    }
}