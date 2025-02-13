// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import { Vm } from "forge-std/Vm.sol";
import { Test } from "forge-std/Test.sol";

import { ISignatureTransfer } from "@uniswap/permit2/src/interfaces/ISignatureTransfer.sol";

import { TestERC20 } from "../mocks/TestERC20.sol";
import { TestValues } from "./TestValues.sol";
import { DeployPermit2 } from "@uniswap/permit2/test/utils/DeployPermit2.sol";

contract IntegrationEnv is Test, DeployPermit2 {
    uint256 private _userCount = 0;
    TestValues private _testValues = new TestValues();

    ISignatureTransfer public immutable _permit2;

    constructor() {
        // Set the default starting balance for the ERC20 token
        _permit2 = ISignatureTransfer(deployPermit2());
    }

    /// @notice Creates a new user and erc20 token, and funds the user with the default amount of tokens
    function createAndFundUser() public returns (address erc20, address user) {
        erc20 = address(new TestERC20());
        user = createAndFundUser(TestERC20(erc20));
        return (erc20, user);
    }

    /// @notice Creates a new user and funds them with the default amount of tokens
    function createAndFundUser(
        TestERC20 token
    ) public returns (address) {
        return createAndFundUser(
            token,
            _testValues.DEFAULT_ERC20_STARTING_BALANCE()
        );
    }

    /// @notice Creates a new user and funds them with the specified amount of tokens
    function createAndFundUser(
        TestERC20 token,
        uint256 amount
    ) public returns (address user) {
        // Create a new user
        user = vm.addr(++_userCount);
        token.mint(user, amount);
    }
}
