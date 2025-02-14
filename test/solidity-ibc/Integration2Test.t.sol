// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

// solhint-disable custom-errors,max-line-length,max-states-count

import { Test } from "forge-std/Test.sol";
import { Vm } from "forge-std/Vm.sol";

import { IICS26RouterMsgs } from "../../contracts/msgs/IICS26RouterMsgs.sol";

import { IERC20 } from "@openzeppelin-contracts/token/ERC20/IERC20.sol";
import { ISignatureTransfer } from "@uniswap/permit2/src/interfaces/ISignatureTransfer.sol";

import { IbcImpl } from "./utils/IbcImpl.sol";
import { TestValues } from "./utils/TestValues.sol";
import { IntegrationEnv } from "./utils/IntegrationEnv.sol";
import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";
import { ICS24Host } from "../../contracts/utils/ICS24Host.sol";

contract IntegrationTest is Test {
    IbcImpl public ibcImplA;
    IbcImpl public ibcImplB;

    TestValues public testValues = new TestValues();
    IntegrationEnv public integrationEnv;

    function setUp() public {
        // Set up the environment
        integrationEnv = new IntegrationEnv();

        // Deploy the IBC implementation
        ibcImplA = new IbcImpl(integrationEnv.permit2());
        ibcImplB = new IbcImpl(integrationEnv.permit2());

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

    function testFuzz_success_sendICS20PacketWithAllowance(uint256 amount) public {
        vm.assume(amount > 0);

        address user = integrationEnv.createAndFundUser(amount);
        address receiver = makeAddr("receiver");

        IICS26RouterMsgs.Packet memory sentPacket =
            ibcImplA.sendTransferAsUser(integrationEnv.erc20(), user, Strings.toHexString(receiver), amount);
        assertEq(integrationEnv.erc20().balanceOf(user), 0, "user balance mismatch");

        // check that the packet was committed correctly
        bytes32 path = ICS24Host.packetCommitmentKeyCalldata(sentPacket.sourceClient, sentPacket.sequence);
        bytes32 expCommitment = ICS24Host.packetCommitmentBytes32(sentPacket);
        bytes32 storedCommitment = ibcImplA.ics26Router().getCommitment(path);
        assertEq(storedCommitment, expCommitment, "packet commitment mismatch");

        // check that the escrow was created and funded correctly
        address escrow = ibcImplA.ics20Transfer().getEscrow(sentPacket.sourceClient);
        assertEq(integrationEnv.erc20().balanceOf(escrow), amount, "escrow balance mismatch");
    }

    function testFuzz_success_sendICS20PacketWithPermit(uint256 amount) public {
        vm.assume(amount > 0);

        address user = integrationEnv.createAndFundUser(amount);
        address receiver = makeAddr("receiver");

        ISignatureTransfer.PermitTransferFrom memory permit;
        bytes memory signature;
        (permit, signature) = integrationEnv.getPermitAndSignature(user, address(ibcImplA.ics20Transfer()), amount);

        IICS26RouterMsgs.Packet memory sentPacket =
            ibcImplA.sendTransferAsUser(integrationEnv.erc20(), user, Strings.toHexString(receiver), permit, signature);
        assertEq(integrationEnv.erc20().balanceOf(user), 0, "user balance mismatch");

        // check that the packet was committed correctly
        bytes32 path = ICS24Host.packetCommitmentKeyCalldata(sentPacket.sourceClient, sentPacket.sequence);
        bytes32 expCommitment = ICS24Host.packetCommitmentBytes32(sentPacket);
        bytes32 storedCommitment = ibcImplA.ics26Router().getCommitment(path);
        assertEq(storedCommitment, expCommitment, "packet commitment mismatch");

        // check that the escrow was created and funded correctly
        address escrow = ibcImplA.ics20Transfer().getEscrow(sentPacket.sourceClient);
        assertEq(integrationEnv.erc20().balanceOf(escrow), amount, "escrow balance mismatch");
    }
}
