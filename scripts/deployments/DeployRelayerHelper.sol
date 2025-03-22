
// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

import { Script } from "forge-std/Script.sol";
import { RelayerHelper } from "../../contracts/utils/RelayerHelper.sol";
import "forge-std/console.sol";

/// @dev See the Solidity Scripting tutorial: https://book.getfoundry.sh/guides/scripting-with-solidity
contract UpgradeICS26 is Script {
    function run() public {
        address ics26Address = vm.promptAddress("Enter the ics26 router address");

        vm.startBroadcast();
        // Deploy new ics26 logic
        address relayerHelper = address(new RelayerHelper(ics26Address));

        vm.stopBroadcast();

        console.log("Deployed Realyer Helper", relayerHelper);
    }
}
