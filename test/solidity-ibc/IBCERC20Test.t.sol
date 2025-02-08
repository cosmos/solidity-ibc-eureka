// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

// solhint-disable custom-errors,max-line-length

import { Test } from "forge-std/Test.sol";

import { IICS20TransferMsgs } from "../../contracts/msgs/IICS20TransferMsgs.sol";

import { IICS20Transfer } from "../../contracts/interfaces/IICS20Transfer.sol";
import { IERC20Errors } from "@openzeppelin-contracts/interfaces/draft-IERC6093.sol";

import { IBCERC20 } from "../../contracts/utils/IBCERC20.sol";
import { Escrow } from "../../contracts/utils/Escrow.sol";
import { ERC1967Proxy } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Proxy.sol";

contract IBCERC20Test is Test, IICS20Transfer {
    IBCERC20 public ibcERC20;
    Escrow public _escrow;
    address public mockICS26;

    function setUp() public {
        mockICS26 = makeAddr("mockICS26");
        address escrowLogic = address(new Escrow());
        _escrow = Escrow(
            address(new ERC1967Proxy(escrowLogic, abi.encodeCall(Escrow.initialize, (address(this), mockICS26))))
        );

        IBCERC20 ibcERC20Logic = new IBCERC20();
        ibcERC20 = IBCERC20(
            address(
                new ERC1967Proxy(
                    address(ibcERC20Logic),
                    abi.encodeCall(
                        ibcERC20Logic.initialize,
                        (address(this), address(_escrow), mockICS26, "test", "full/denom/path/test")
                    )
                )
            )
        );
    }

    function test_ERC20Metadata() public view {
        assertEq(ibcERC20.ics20(), address(this));
        assertEq(ibcERC20.escrow(), address(_escrow));
        assertEq(ibcERC20.ics26(), mockICS26);
        assertEq(ibcERC20.name(), "full/denom/path/test");
        assertEq(ibcERC20.symbol(), "test");
        assertEq(ibcERC20.fullDenomPath(), "full/denom/path/test");
        assertEq(0, ibcERC20.totalSupply());
    }

    function test_EscrowSetup() public view {
        assertEq(_escrow.ics20(), address(this));
        assertEq(_escrow.ics26(), mockICS26);
    }

    function testFuzz_success_Mint(uint256 amount) public {
        ibcERC20.mint(amount);
        assertEq(ibcERC20.balanceOf(address(_escrow)), amount);
        assertEq(ibcERC20.totalSupply(), amount);
    }

    // Just to document the behaviour
    function test_MintZero() public {
        ibcERC20.mint(0);
        assertEq(ibcERC20.balanceOf(address(_escrow)), 0);
        assertEq(ibcERC20.totalSupply(), 0);
    }

    function testFuzz_unauthorized_Mint(uint256 amount) public {
        address notICS20Transfer = makeAddr("notICS20Transfer");
        vm.expectRevert(abi.encodeWithSelector(IBCERC20.IBCERC20Unauthorized.selector, notICS20Transfer));
        vm.prank(notICS20Transfer);
        ibcERC20.mint(amount);
        assertEq(ibcERC20.balanceOf(notICS20Transfer), 0);
        assertEq(ibcERC20.balanceOf(address(_escrow)), 0);
        assertEq(ibcERC20.totalSupply(), 0);
    }

    function testFuzz_success_Burn(uint256 startingAmount, uint256 burnAmount) public {
        burnAmount = bound(burnAmount, 0, startingAmount);
        ibcERC20.mint(startingAmount);
        assertEq(ibcERC20.balanceOf(address(_escrow)), startingAmount);

        ibcERC20.burn(burnAmount);
        uint256 leftOver = startingAmount - burnAmount;
        assertEq(ibcERC20.balanceOf(address(_escrow)), leftOver);
        assertEq(ibcERC20.totalSupply(), leftOver);

        if (leftOver != 0) {
            ibcERC20.burn(leftOver);
            assertEq(ibcERC20.balanceOf(address(_escrow)), 0);
            assertEq(ibcERC20.totalSupply(), 0);
        }
    }

    function testFuzz_unauthorized_Burn(uint256 startingAmount, uint256 burnAmount) public {
        burnAmount = bound(burnAmount, 0, startingAmount);
        ibcERC20.mint(startingAmount);
        assertEq(ibcERC20.balanceOf(address(_escrow)), startingAmount);

        address notICS20Transfer = makeAddr("notICS20Transfer");
        vm.expectRevert(abi.encodeWithSelector(IBCERC20.IBCERC20Unauthorized.selector, notICS20Transfer));
        vm.prank(notICS20Transfer);
        ibcERC20.burn(burnAmount);
        assertEq(ibcERC20.balanceOf(notICS20Transfer), 0);
        assertEq(ibcERC20.balanceOf(address(_escrow)), startingAmount);
        assertEq(ibcERC20.totalSupply(), startingAmount);
    }

    // Just to document the behaviour
    function test_BurnZero() public {
        ibcERC20.burn(0);
        assertEq(ibcERC20.balanceOf(address(_escrow)), 0);
        assertEq(ibcERC20.totalSupply(), 0);

        ibcERC20.mint(1000);
        ibcERC20.burn(0);
        assertEq(ibcERC20.balanceOf(address(_escrow)), 1000);
        assertEq(ibcERC20.totalSupply(), 1000);
    }

    function test_failure_Burn() public {
        // test burn with zero balance
        vm.expectRevert(abi.encodeWithSelector(IERC20Errors.ERC20InsufficientBalance.selector, address(_escrow), 0, 1));
        ibcERC20.burn(1);

        // mint some to test other cases
        ibcERC20.mint(1000);

        // test burn with insufficient balance
        vm.expectRevert(
            abi.encodeWithSelector(IERC20Errors.ERC20InsufficientBalance.selector, address(_escrow), 1000, 1001)
        );
        ibcERC20.burn(1001);
    }

    // Dummy implementation of IICS20Transfer
    function sendTransfer(IICS20TransferMsgs.SendTransferMsg calldata) external pure returns (uint32 sequence) {
        return 0;
    }

    // Dummy implementation of IICS20Transfer
    function escrow() external view override returns (address) {
        return address(_escrow);
    }

    // Dummy implementation of IICS20Transfer
    function ibcERC20Contract(string calldata) external pure override returns (address) {
        return address(0);
    }

    /// @notice Dummy implementation of IICS20Transfer
    function initialize(address, address, address, address) external pure override { }
    // solhint-disable-previous-line no-empty-blocks
}
