// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

// solhint-disable custom-errors,max-line-length,max-states-count

import { Test } from "forge-std/Test.sol";
import { Vm } from "forge-std/Vm.sol";

import { IbcImpl } from "./utils/IbcImpl.sol";
import { TestValues } from "./utils/TestValues.sol";

contract IntegrationTest is Test {
    IbcImpl public ibcImplA;
    IbcImpl public ibcImplB;

    TestValues public testValues = new TestValues();

    function setUp() public {
        // Deploy the IBC implementation
        ibcImplA = new IbcImpl(address(0));
        ibcImplB = new IbcImpl(address(0));

        // Add the counterparty implementations
        string memory clientId;
        clientId = ibcImplA.addCounterpartyImpl(ibcImplB, testValues.FIRST_CLIENT_ID());
        assertEq(clientId, testValues.FIRST_CLIENT_ID());

        clientId = ibcImplB.addCounterpartyImpl(ibcImplA, testValues.FIRST_CLIENT_ID());
        assertEq(clientId, testValues.FIRST_CLIENT_ID());
    }

    function test_deployment() public view {
        // Check that the counterparty implementations are set correctly
        assertEq(
            ibcImplA.ics26Router().getClient(testValues.FIRST_CLIENT_ID()).getClientState(),
            abi.encodePacked(address(ibcImplB.ics26Router()))
        );
        assertEq(
            ibcImplB.ics26Router().getClient(testValues.FIRST_CLIENT_ID()).getClientState(),
            abi.encodePacked(address(ibcImplA.ics26Router()))
        );
    }
}
