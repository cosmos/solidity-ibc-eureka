// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

// solhint-disable custom-errors,max-line-length,max-states-count

import { Test } from "forge-std/Test.sol";
import { Vm } from "forge-std/Vm.sol";

import { IICS26RouterMsgs } from "../../contracts/msgs/IICS26RouterMsgs.sol";
import { IICS20TransferMsgs } from "../../contracts/msgs/IICS20TransferMsgs.sol";

import { IERC20 } from "@openzeppelin-contracts/token/ERC20/IERC20.sol";
import { ISignatureTransfer } from "@uniswap/permit2/src/interfaces/ISignatureTransfer.sol";
import { IICS20Transfer } from "../../contracts/interfaces/IICS20Transfer.sol";
import { IICS26Router } from "../../contracts/interfaces/IICS26Router.sol";
import { IICS26RouterErrors } from "../../contracts/errors/IICS26RouterErrors.sol";

import { IbcImpl } from "./utils/IbcImpl.sol";
import { TestHelper } from "./utils/TestHelper.sol";
import { IntegrationEnv } from "./utils/IntegrationEnv.sol";
import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";
import { ICS24Host } from "../../contracts/utils/ICS24Host.sol";
import { ICS20Lib } from "../../contracts/utils/ICS20Lib.sol";

contract IntegrationTest is Test {
    IbcImpl public ibcImplA;
    IbcImpl public ibcImplB;

    TestHelper public testHelper = new TestHelper();
    IntegrationEnv public integrationEnv;

    function setUp() public {
        // Set up the environment
        integrationEnv = new IntegrationEnv();

        // Deploy the IBC implementation
        ibcImplA = new IbcImpl(integrationEnv.permit2());
        ibcImplB = new IbcImpl(integrationEnv.permit2());

        // Add the counterparty implementations
        string memory clientId;
        clientId = ibcImplA.addCounterpartyImpl(ibcImplB, testHelper.FIRST_CLIENT_ID());
        assertEq(clientId, testHelper.FIRST_CLIENT_ID());

        clientId = ibcImplB.addCounterpartyImpl(ibcImplA, testHelper.FIRST_CLIENT_ID());
        assertEq(clientId, testHelper.FIRST_CLIENT_ID());
    }

    function test_deployment() public view {
        // Check that the counterparty implementations are set correctly
        assertEq(
            ibcImplA.ics26Router().getClient(testHelper.FIRST_CLIENT_ID()).getClientState(),
            abi.encodePacked(address(ibcImplB.ics26Router()))
        );
        assertEq(
            ibcImplB.ics26Router().getClient(testHelper.FIRST_CLIENT_ID()).getClientState(),
            abi.encodePacked(address(ibcImplA.ics26Router()))
        );
    }

    function testFuzz_success_sendICS20PacketWithAllowance(uint256 amount) public {
        vm.assume(amount > 0);

        address user = integrationEnv.createAndFundUser(amount);
        string memory receiver = testHelper.randomString();

        IICS26RouterMsgs.Packet memory sentPacket =
            ibcImplA.sendTransferAsUser(integrationEnv.erc20(), user, receiver, amount);
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
        string memory receiver = testHelper.randomString();

        ISignatureTransfer.PermitTransferFrom memory permit;
        bytes memory signature;
        (permit, signature) = integrationEnv.getPermitAndSignature(user, address(ibcImplA.ics20Transfer()), amount);

        IICS26RouterMsgs.Packet memory sentPacket =
            ibcImplA.sendTransferAsUser(integrationEnv.erc20(), user, receiver, permit, signature);
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

    function testFuzz_success_recvNativeICS20Packet(uint256 amount) public {
        // We will send a packet from A to B and then receive it on B
        vm.assume(amount > 0);

        address user = integrationEnv.createAndFundUser(amount);
        address receiver = integrationEnv.createUser();

        IICS26RouterMsgs.Packet memory sentPacket =
            ibcImplA.sendTransferAsUser(integrationEnv.erc20(), user, Strings.toHexString(receiver), amount);

        // Receive the packet on B
        bytes[] memory acks = ibcImplB.recvPacket(sentPacket);
        assertEq(acks.length, 1, "ack length mismatch");
        assertEq(acks, testHelper.SINGLE_SUCCESS_ACK(), "ack mismatch");

        // Verify that the packet acknowledgement was written correctly
        bytes32 path = ICS24Host.packetAcknowledgementCommitmentKeyCalldata(sentPacket.destClient, sentPacket.sequence);
        bytes32 expAckCommitment = ICS24Host.packetAcknowledgementCommitmentBytes32(testHelper.SINGLE_SUCCESS_ACK());
        bytes32 storedAckCommitment = ibcImplB.ics26Router().getCommitment(path);
        assertEq(storedAckCommitment, expAckCommitment, "ack commitment mismatch");

        // Verify that the packet receipt was set correctly
        bytes32 receiptPath =
            keccak256(ICS24Host.packetReceiptCommitmentPathCalldata(sentPacket.destClient, sentPacket.sequence));
        bytes32 expReceipt = ICS24Host.packetReceiptCommitmentBytes32(sentPacket);
        bytes32 storedReceipt = ibcImplB.ics26Router().getCommitment(receiptPath);
        assertEq(storedReceipt, expReceipt, "receipt mismatch");

        // Check that a new IBCERC20 token was created
        string memory expDenomPath = string.concat(
            ICS20Lib.DEFAULT_PORT_ID,
            "/",
            testHelper.FIRST_CLIENT_ID(),
            "/",
            Strings.toHexString(address(integrationEnv.erc20()))
        );
        IERC20 token = IERC20(ibcImplB.ics20Transfer().ibcERC20Contract(expDenomPath));
        assertTrue(address(token) != address(0), "IBCERC20 token not found");
        assertEq(token.balanceOf(receiver), amount, "receiver balance mismatch");

        // Check replay protection
        IICS26RouterMsgs.MsgRecvPacket memory msgRecvPacket;
        msgRecvPacket.packet = sentPacket;
        vm.recordLogs();
        ibcImplB.ics26Router().recvPacket(msgRecvPacket);
        testHelper.getValueFromEvent(IICS26Router.Noop.selector);
    }

    function testFuzz_success_ackPacket(uint256 amount) public {
        // We will send a packet from A to B and then receive it on B
        vm.assume(amount > 0);

        address user = integrationEnv.createAndFundUser(amount);
        address receiver = integrationEnv.createUser();

        IICS26RouterMsgs.Packet memory sentPacket =
            ibcImplA.sendTransferAsUser(integrationEnv.erc20(), user, Strings.toHexString(receiver), amount);
        bytes[] memory acks = ibcImplB.recvPacket(sentPacket);

        // Acknowledge the packet on A
        ibcImplA.ackPacket(sentPacket, acks);

        // Verify that the packet commitment was deleted
        bytes32 path = ICS24Host.packetCommitmentKeyCalldata(sentPacket.sourceClient, sentPacket.sequence);
        bytes32 storedCommitment = ibcImplA.ics26Router().getCommitment(path);
        assertTrue(storedCommitment == 0, "packet commitment not deleted");

        // Verify that the tokens were transferred
        assertEq(integrationEnv.erc20().balanceOf(user), 0, "user balance mismatch");

        // Check replay protection
        vm.expectEmit();
        emit IICS26Router.Noop();
        ibcImplA.ackPacket(sentPacket, acks);
    }

    function testFuzz_success_timeoutPacket(uint256 amount) public {
        // We will send a packet from A to B and then time it out on A
        vm.assume(amount > 0);

        address user = integrationEnv.createAndFundUser(amount);
        address receiver = integrationEnv.createUser();

        IICS26RouterMsgs.Packet memory sentPacket =
            ibcImplA.sendTransferAsUser(integrationEnv.erc20(), user, Strings.toHexString(receiver), amount, uint64(block.timestamp + 10 seconds));

        // Set the block timestamp to the timeout
        vm.warp(block.timestamp + 30 seconds);

        // Fail to receive the packet on Chain B
        vm.expectRevert(abi.encodeWithSelector(IICS26RouterErrors.IBCInvalidTimeoutTimestamp.selector, sentPacket.timeoutTimestamp, block.timestamp));
        ibcImplB.recvPacket(sentPacket);

        // Timeout the packet on Chain A
        ibcImplA.timeoutPacket(sentPacket);

        // commitment should be deleted
        bytes32 storedCommitment = ibcImplA.relayerHelper().queryPacketCommitment(sentPacket.sourceClient, sentPacket.sequence);
        assertEq(storedCommitment, 0);

        // transfer should be reverted
        uint256 senderBalanceAfterTimeout = integrationEnv.erc20().balanceOf(user);
        uint256 contractBalanceAfterTimeout = integrationEnv.erc20().balanceOf(ibcImplA.ics20Transfer().getEscrow(testHelper.FIRST_CLIENT_ID()));
        assertEq(senderBalanceAfterTimeout, amount);
        assertEq(contractBalanceAfterTimeout, 0);

        // Check replay protection
        vm.expectEmit();
        emit IICS26Router.Noop();
        ibcImplA.timeoutPacket(sentPacket);
    }
}
