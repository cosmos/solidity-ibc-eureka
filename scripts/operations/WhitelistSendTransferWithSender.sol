
// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

import { Script } from "forge-std/Script.sol";
import { ICS20Transfer } from "../../contracts/ICS20Transfer.sol";
import "forge-std/console.sol";

/// @dev See the Solidity Scripting tutorial: https://book.getfoundry.sh/guides/scripting-with-solidity
contract WhitelistSendTransferWithSender is Script {
    function run() public {
        address ics20Address = vm.promptAddress("Enter the ics20 router address");
        address delegateAddress = vm.promptAddress("Enter the delegate address");
        ICS20Transfer ics20 = ICS20Transfer(ics20Address);

        vm.startBroadcast();

        ics20.grantDelegateSenderRole(delegateAddress);

        vm.stopBroadcast();

        console.log("Whitelisted delegate to send transfer with sender", delegateAddress);
    }
}
