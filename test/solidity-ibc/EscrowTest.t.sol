// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

// solhint-disable max-line-length,gas-custom-errors

import { console } from "forge-std/console.sol";
import { Test } from "forge-std/Test.sol";

import { IIBCUUPSUpgradeable } from "../../contracts/interfaces/IIBCUUPSUpgradeable.sol";
import { IEscrowErrors } from "../../contracts/errors/IEscrowErrors.sol";
import { IRateLimitErrors } from "../../contracts/errors/IRateLimitErrors.sol";
import { IAccessControl } from "@openzeppelin-contracts/access/AccessControl.sol";
import { IERC20 } from "@openzeppelin-contracts/token/ERC20/IERC20.sol";

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

    function test_failure_setRateLimiterRole() public {
        address newRateLimiter = makeAddr("newRateLimiter");

        vm.mockCall(mockICS26, abi.encodeWithSelector(IIBCUUPSUpgradeable.isAdmin.selector), abi.encode(false));
        vm.expectRevert(abi.encodeWithSelector(IEscrowErrors.EscrowUnauthorized.selector, address(this)));
        escrow.grantRateLimiterRole(newRateLimiter);

        vm.mockCall(mockICS26, abi.encodeWithSelector(IIBCUUPSUpgradeable.isAdmin.selector), abi.encode(false));
        vm.expectRevert(abi.encodeWithSelector(IEscrowErrors.EscrowUnauthorized.selector, address(this)));
        escrow.revokeRateLimiterRole(rateLimiter);
    }

    function test_success_setRateLimit() public {
        address mockToken = makeAddr("mockToken");
        uint256 rateLimit = 10_000;

        vm.prank(rateLimiter);
        escrow.setRateLimit(mockToken, rateLimit);
        assertEq(escrow.getRateLimit(mockToken), rateLimit);
    }

    function test_failure_setRateLimit() public {
        address mockToken = makeAddr("mockToken");
        uint256 rateLimit = 10_000;

        vm.expectRevert(abi.encodeWithSelector(IAccessControl.AccessControlUnauthorizedAccount.selector, address(this), escrow.RATE_LIMITER_ROLE()));
        escrow.setRateLimit(mockToken, rateLimit);
        assertEq(escrow.getRateLimit(mockToken), 0);
    }

    function test_dailyUsage() public {
        address mockToken = makeAddr("mockToken");

        vm.mockCall(mockToken, abi.encodeWithSelector(IERC20.transferFrom.selector), abi.encode(true));
        escrow.send(IERC20(mockToken), address(this), 10_000);
        assertEq(escrow.getDailyUsage(mockToken), 10_000);

        vm.mockCall(mockToken, abi.encodeWithSelector(IERC20.transferFrom.selector), abi.encode(true));
        escrow.send(IERC20(mockToken), address(this), 1_020);
        assertEq(escrow.getDailyUsage(mockToken), 11_020);

        vm.warp(block.timestamp + 1 days);
        assertEq(escrow.getDailyUsage(mockToken), 0);

        vm.mockCall(mockToken, abi.encodeWithSelector(IERC20.transferFrom.selector), abi.encode(true));
        escrow.send(IERC20(mockToken), address(this), 100_000);
        assertEq(escrow.getDailyUsage(mockToken), 100_000);
    }

    /// forge-config: default.fuzz.runs = 1000
    function testFuzz_rateLimit(uint16 n) public {
        vm.assume(1 < n);
        vm.assume(n < 1000);

        address mockToken = makeAddr("mockToken");
        uint256 sendAmount = 10_000;
        uint256 rateLimit = sendAmount * n - 1;

        vm.prank(rateLimiter);
        escrow.setRateLimit(mockToken, rateLimit);

        for (uint256 i = 0; i < n - 1; i++) {
            vm.mockCall(mockToken, abi.encodeWithSelector(IERC20.transferFrom.selector), abi.encode(true));
            escrow.send(IERC20(mockToken), address(this), sendAmount);
            assertEq(escrow.getDailyUsage(mockToken), sendAmount * (i + 1));
        }

        vm.mockCall(mockToken, abi.encodeWithSelector(IERC20.transferFrom.selector), abi.encode(true));
        vm.expectRevert(abi.encodeWithSelector(IRateLimitErrors.RateLimitExceeded.selector, rateLimit, sendAmount * n));
        escrow.send(IERC20(mockToken), address(this), sendAmount);
    }
}
