// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

import "../../contracts/ICS20Transfer.sol";
import {Script} from "forge-std/Script.sol";
import {stdJson} from "forge-std/StdJson.sol";

contract ICS20TransferDeployScript is Script {
    using stdJson for string;

    function deploy() public returns (address) {
        ICS20Transfer ics20Transfer = new ICS20Transfer();

        return address(ics20Transfer);
    }

    function run() public returns (address) {
        return deploy();
    }
}