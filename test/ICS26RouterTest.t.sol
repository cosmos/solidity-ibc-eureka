// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.25 <0.9.0;

// solhint-disable custom-errors,max-line-length

import { Test } from "forge-std/Test.sol";
import { ICS02Client } from "../src/ICS02Client.sol";
import { ICS26Router } from "../src/ICS26Router.sol";
import { IICS26Router } from "../src/interfaces/IICS26Router.sol";
import { ICS20Transfer } from "../src/ICS20Transfer.sol";
import { Strings } from "@openzeppelin/contracts/utils/Strings.sol";

contract ICS26RouterTest is Test {
    ICS02Client public ics02Client;
    ICS26Router public ics26Router;

    function setUp() public {
        ics02Client = new ICS02Client(address(this));
        ics26Router = new ICS26Router(address(ics02Client), address(this));
    }

    function test_AddIBCAppUsingAddress() public {
        ICS20Transfer ics20Transfer = new ICS20Transfer(address(ics26Router));
        string memory ics20AddressStr = Strings.toHexString(address(ics20Transfer));

        vm.expectEmit();
        emit IICS26Router.IBCAppAdded(ics20AddressStr, address(ics20Transfer));
        ics26Router.addIBCApp("", address(ics20Transfer));

        assertEq(address(ics20Transfer), address(ics26Router.getIBCApp(ics20AddressStr)));
    }

    function test_AddIBCAppUsingNamedPort() public {
       ICS20Transfer ics20Transfer = new ICS20Transfer(address(ics26Router));

        vm.expectEmit();
        emit IICS26Router.IBCAppAdded("transfer", address(ics20Transfer));
        ics26Router.addIBCApp("transfer", address(ics20Transfer));

        assertEq(address(ics20Transfer), address(ics26Router.getIBCApp("transfer")));
    }
}
