// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.25 <0.9.0;

// solhint-disable custom-errors,max-line-length

import { Test } from "forge-std/Test.sol";
import { IBCERC20 } from "../src/utils/IBCERC20.sol";
import { IICS20Transfer } from "../src/interfaces/IICS20Transfer.sol";
import { Ownable } from "@openzeppelin/contracts/access/Ownable.sol";
import { IERC20Errors } from "@openzeppelin/contracts/interfaces/draft-IERC6093.sol";

contract IBCERC20Test is Test, IICS20Transfer {
    IBCERC20 public ibcERC20;

    function setUp() public {
        ibcERC20 = new IBCERC20(IICS20Transfer(this), "ibc/test", "test", "full/denom/path/test");
    }

    function test_ERC20Metadata() public view {
        assertEq(ibcERC20.owner(), address(this));
        assertEq(ibcERC20.name(), "ibc/test");
        assertEq(ibcERC20.symbol(), "test");
        assertEq(ibcERC20.fullDenomPath(), "full/denom/path/test");
        assertEq(0, ibcERC20.totalSupply());
    }

    function testFuzz_success_Mint(uint256 amount) public {
        ibcERC20.mint(address(this), amount);
        assertEq(ibcERC20.balanceOf(address(this)), amount);
        assertEq(ibcERC20.totalSupply(), amount);
    }

    // Just to document the behaviour
    function test_MintZero() public {
        ibcERC20.mint(address(this), 0);
        assertEq(ibcERC20.balanceOf(address(this)), 0);
        assertEq(ibcERC20.totalSupply(), 0);
    }

    function testFuzz_unauthorized_Mint(uint256 amount) public {
        address notICS20Transfer = makeAddr("notICS20Transfer");
        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, notICS20Transfer));
        vm.prank(notICS20Transfer);
        ibcERC20.mint(address(this), amount);
        assertEq(ibcERC20.balanceOf(notICS20Transfer), 0);
        assertEq(ibcERC20.balanceOf(address(this)), 0);
        assertEq(ibcERC20.totalSupply(), 0);
    }

    function testFuzz_success_Burn(uint256 startingAmount, uint256 burnAmount) public {
        burnAmount = bound(burnAmount, 0, startingAmount);
        ibcERC20.mint(address(this), startingAmount);
        assertEq(ibcERC20.balanceOf(address(this)), startingAmount);

        ibcERC20.burn(address(this), burnAmount);
        uint256 leftOver = startingAmount - burnAmount;
        assertEq(ibcERC20.balanceOf(address(this)), leftOver);
        assertEq(ibcERC20.totalSupply(), leftOver);

        if (leftOver != 0) {
            ibcERC20.burn(address(this), leftOver);
            assertEq(ibcERC20.balanceOf(address(this)), 0);
            assertEq(ibcERC20.totalSupply(), 0);
        }
    }

    function testFuzz_unauthorized_Burn(uint256 startingAmount, uint256 burnAmount) public {
        burnAmount = bound(burnAmount, 0, startingAmount);
        ibcERC20.mint(address(this), startingAmount);
        assertEq(ibcERC20.balanceOf(address(this)), startingAmount);

        address notICS20Transfer = makeAddr("notICS20Transfer");
        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, notICS20Transfer));
        vm.prank(notICS20Transfer);
        ibcERC20.burn(address(this), burnAmount);
        assertEq(ibcERC20.balanceOf(notICS20Transfer), 0);
        assertEq(ibcERC20.balanceOf(address(this)), startingAmount);
        assertEq(ibcERC20.totalSupply(), startingAmount);
    }

    // Just to document the behaviour
    function test_BurnZero() public {
        ibcERC20.burn(address(this), 0);
        assertEq(ibcERC20.balanceOf(address(this)), 0);
        assertEq(ibcERC20.totalSupply(), 0);

        ibcERC20.mint(address(this), 1000);
        ibcERC20.burn(address(this), 0);
        assertEq(ibcERC20.balanceOf(address(this)), 1000);
        assertEq(ibcERC20.totalSupply(), 1000);
    }

    function test_failure_Burn() public {
        // test burn with zero balance
        vm.expectRevert(abi.encodeWithSelector(IERC20Errors.ERC20InsufficientBalance.selector, address(this), 0, 1));
        ibcERC20.burn(address(this), 1);

        // mint some to test other cases
        ibcERC20.mint(address(this), 1000);

        // test burn with insufficient balance
        vm.expectRevert(
            abi.encodeWithSelector(IERC20Errors.ERC20InsufficientBalance.selector, address(this), 1000, 1001)
        );
        ibcERC20.burn(address(this), 1001);
    }

    // Dummy implementation of IICS20Transfer
    function sendTransfer(SendTransferMsg calldata) external pure returns (uint32 sequence) {
        return 0;
    }
}
