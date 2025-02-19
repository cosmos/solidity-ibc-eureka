// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

import { Script } from "forge-std/Script.sol";
import { ICS26Router } from "../contracts/ICS26Router.sol";
import "forge-std/console.sol";

/// @dev See the Solidity Scripting tutorial: https://book.getfoundry.sh/guides/scripting-with-solidity
contract UpgradeICS26 is Script {
    function run() public {
        address ics26Address = vm.promptAddress("Enter the ics26 router address");
        ICS26Router existingICS26 = ICS26Router(ics26Address);

        vm.startBroadcast();
        // Deploy new ics26 logic
        address newICS26Logic = address(new ICS26Router());

        // Upgrade the ics26 router
        existingICS26.upgradeToAndCall(newICS26Logic, bytes(""));

        vm.stopBroadcast();

        console.log("Upgraded ICS26Router");
    }
}
