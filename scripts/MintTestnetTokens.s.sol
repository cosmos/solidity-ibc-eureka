// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

/*
    This script is used for deploying auxiliary contracts to a live network (it could be local, but is geared towards testnet)
*/

import { Script } from "forge-std/Script.sol";
import { TestnetERC20 } from "./TestnetERC20.sol";
import "forge-std/console.sol";

/// @dev See the Solidity Scripting tutorial: https://book.getfoundry.sh/guides/scripting-with-solidity
contract MintTestnetTokens is Script {
    function run() public {
        address to = vm.promptAddress("Enter the address to mint tokens to");
        uint256 amount = vm.promptUint("Enter the amount of tokens to mint");
        TestnetERC20 testnetERC20 = TestnetERC20(address(0xA4ff49eb6E2Ea77d7D8091f1501385078642603f));

        vm.startBroadcast();
        testnetERC20.mint(to, amount);
        vm.stopBroadcast();

        console.log("Minted", amount, "tokens to", to);
    }
}
