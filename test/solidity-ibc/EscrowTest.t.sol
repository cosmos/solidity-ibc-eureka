// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

// solhint-disable max-line-length,gas-custom-errors

import { Test } from "forge-std/Test.sol";

import { Escrow } from "../../contracts/utils/Escrow.sol";
import { ERC1967Proxy } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Proxy.sol";

contract EscrowTest is Test {
    address public mockIcs26 = makeAddr("mockIcs26");
    Escrow public escrow;

    function setUp() public {
        // setup code here
        address escrowLogic = address(new Escrow());

        ERC1967Proxy escrowProxy = new ERC1967Proxy(
            escrowLogic,
            abi.encodeWithSelector(Escrow.initialize.selector, address(this), mockIcs26)
        );
        escrow = Escrow(address(escrowProxy));
    }
}
