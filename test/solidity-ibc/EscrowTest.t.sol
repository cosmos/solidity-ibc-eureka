// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

// solhint-disable max-line-length,gas-custom-errors

import { Test } from "forge-std/Test.sol";

import { IIBCUUPSUpgradeable } from "../../contracts/interfaces/IIBCUUPSUpgradeable.sol";

import { Escrow } from "../../contracts/utils/Escrow.sol";
import { ERC1967Proxy } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Proxy.sol";

contract EscrowTest is Test {
    address public mockICS26 = makeAddr("mockICS26");
    address public rateLimiter = makeAddr("rateLimiter");
    Escrow public escrow;

    function setUp() public {
        // setup code here
        address escrowLogic = address(new Escrow());

        ERC1967Proxy escrowProxy = new ERC1967Proxy(
            escrowLogic,
            abi.encodeWithSelector(Escrow.initialize.selector, address(this), mockICS26)
        );
        escrow = Escrow(address(escrowProxy));

        // Have admin approval for next call
        vm.mockCall(mockICS26, abi.encodeWithSelector(IIBCUUPSUpgradeable.isAdmin.selector), abi.encode(true));
        // Set rate limiter role
        escrow.grantRateLimiterRole(rateLimiter);
        assertTrue(escrow.hasRole(escrow.RATE_LIMITER_ROLE(), rateLimiter));
    }

    function test_success_setRateLimiterRole() public {
        address newRateLimiter = makeAddr("newRateLimiter");

        vm.mockCall(mockICS26, abi.encodeWithSelector(IIBCUUPSUpgradeable.isAdmin.selector), abi.encode(true));
        escrow.grantRateLimiterRole(newRateLimiter);
        assertTrue(escrow.hasRole(escrow.RATE_LIMITER_ROLE(), newRateLimiter));

        vm.mockCall(mockICS26, abi.encodeWithSelector(IIBCUUPSUpgradeable.isAdmin.selector), abi.encode(true));
        escrow.revokeRateLimiterRole(newRateLimiter);
        assertFalse(escrow.hasRole(escrow.RATE_LIMITER_ROLE(), newRateLimiter));
    }
}
