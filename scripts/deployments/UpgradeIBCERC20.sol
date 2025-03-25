// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

import { Script } from "forge-std/Script.sol";
import { IBCERC20 } from "../../contracts/utils/IBCERC20.sol";
import { ICS20Transfer } from "../../contracts/ICS20Transfer.sol";
import "forge-std/console.sol";

/// @dev See the Solidity Scripting tutorial: https://book.getfoundry.sh/guides/scripting-with-solidity
contract UpgradeIBCERC20 is Script {
    function run() public {
        address ics20Address = vm.promptAddress("Enter the ics20 transfer address");
        ICS20Transfer ics20 = ICS20Transfer(ics20Address);

        vm.startBroadcast();
        // Deploy new ibcerc20 logic
        address newIBCERC20Logic = address(new IBCERC20());

        // Upgrade the beacon proxy
        ics20.upgradeIBCERC20To(newIBCERC20Logic);

        vm.stopBroadcast();

        console.log("Upgraded ICS26Router");
    }
}
