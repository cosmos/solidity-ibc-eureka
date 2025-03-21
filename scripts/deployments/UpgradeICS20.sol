// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

import { Script } from "forge-std/Script.sol";
import { ICS20Transfer } from "../../contracts/ICS20Transfer.sol";
import "forge-std/console.sol";

/// @dev See the Solidity Scripting tutorial: https://book.getfoundry.sh/guides/scripting-with-solidity
contract UpgradeICS20 is Script {
    function run() public {
        address ics20Address = vm.promptAddress("Enter the ics20 transfer address");
        ICS20Transfer existingICS20 = ICS20Transfer(ics20Address);

        vm.startBroadcast();
        // Deploy new ics26 logic
        address newICS20Logic = address(new ICS20Transfer());

        // Upgrade the ics26 router
        existingICS20.upgradeToAndCall(newICS20Logic, bytes(""));

        vm.stopBroadcast();

        console.log("Upgraded ICS26Router");
    }
}


