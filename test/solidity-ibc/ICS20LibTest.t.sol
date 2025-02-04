// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

// solhint-disable custom-errors,max-line-length

import { Test } from "forge-std/Test.sol";
import { ICS20Lib } from "../../contracts/utils/ICS20Lib.sol";

contract ICS20LibTest is Test {
    function test_addHop() public pure {
        bytes memory baseDenom = "uatom";
        bytes memory withHop = ICS20Lib.addHop(baseDenom, "transfer/client-0/");
        assertEq(string(withHop), "transfer/client-0/uatom");

        bytes memory withHop2 = ICS20Lib.addHop(withHop, "transfer/client-1/");
        assertEq(string(withHop2), "transfer/client-1/transfer/client-0/uatom");
    }

    function test_removeHop() public pure {
        bytes memory denomWithHop = "transfer/client-0/uatom";
        bytes memory withoutHop = ICS20Lib.removeHop(denomWithHop, "transfer/client-0/");
        assertEq(string(withoutHop), "uatom");
    }

    function test_hasHops() public pure {
        assert(ICS20Lib.hasHops("transfer/client-0/uatom"));
        assert(!ICS20Lib.hasHops("uatom"));
    }
}
