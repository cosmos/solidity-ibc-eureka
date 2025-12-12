// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

// solhint-disable custom-errors,max-line-length,max-states-count,var-name-mixedcase,gas-small-strings

import { Test } from "forge-std/Test.sol";

import { IICS26RouterMsgs } from "../../contracts/msgs/IICS26RouterMsgs.sol";
import { IICS26Router } from "../../contracts/interfaces/IICS26Router.sol";
import { IIFTMsgs } from "../../contracts/msgs/IIFTMsgs.sol";

import { IbcImpl } from "./utils/IbcImpl.sol";
import { TestHelper } from "./utils/TestHelper.sol";
import { IntegrationEnv } from "./utils/IntegrationEnv.sol";

import { IFTBase } from "../../contracts/IFTBase.sol";
import { EVMIFTSendCallConstructor } from "../../contracts/utils/EVMIFTSendCallConstructor.sol";
import { IICS27GMP } from "../../contracts/interfaces/IICS27GMP.sol";
import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";
import { ERC20 } from "@openzeppelin-contracts/token/ERC20/ERC20.sol";

/// @title TestIFT - A concrete IFT implementation for integration testing
contract TestIFT is IFTBase {
    constructor(
        IICS27GMP ics27Gmp_,
        address authority_
    )
        ERC20("Test Interchain Token", "TIFT")
        IFTBase(ics27Gmp_, authority_)
    { }

    /// @notice Mint tokens for testing purposes
    function mint(address to, uint256 amount) external {
        _mint(to, amount);
    }
}

contract IFTIntegrationTest is Test {
    IbcImpl public ibcImplA;
    IbcImpl public ibcImplB;

    TestHelper public th = new TestHelper();
    IntegrationEnv public integrationEnv;

    TestIFT public iftOnA;
    TestIFT public iftOnB;
    EVMIFTSendCallConstructor public sendCallConstructor;

    function setUp() public {
        integrationEnv = new IntegrationEnv();

        ibcImplA = new IbcImpl(integrationEnv.permit2());
        ibcImplB = new IbcImpl(integrationEnv.permit2());

        string memory clientId;
        clientId = ibcImplA.addCounterpartyImpl(ibcImplB, th.FIRST_CLIENT_ID());
        assertEq(clientId, th.FIRST_CLIENT_ID());

        clientId = ibcImplB.addCounterpartyImpl(ibcImplA, th.FIRST_CLIENT_ID());
        assertEq(clientId, th.FIRST_CLIENT_ID());

        sendCallConstructor = new EVMIFTSendCallConstructor();

        iftOnA = new TestIFT(ibcImplA.ics27Gmp(), address(ibcImplA.accessManager()));
        iftOnB = new TestIFT(ibcImplB.ics27Gmp(), address(ibcImplB.accessManager()));

        _setupBridgePermissions();
        _registerBridges();
    }

    function _setupBridgePermissions() internal {
        uint64 ADMIN_ROLE = 0;

        bytes4[] memory selectors = new bytes4[](1);
        selectors[0] = IFTBase.registerIFTBridge.selector;

        ibcImplA.accessManager().setTargetFunctionRole(address(iftOnA), selectors, ADMIN_ROLE);
        ibcImplB.accessManager().setTargetFunctionRole(address(iftOnB), selectors, ADMIN_ROLE);
    }

    function _registerBridges() internal {
        iftOnA.registerIFTBridge(
            th.FIRST_CLIENT_ID(), Strings.toChecksumHexString(address(iftOnB)), address(sendCallConstructor)
        );

        iftOnB.registerIFTBridge(
            th.FIRST_CLIENT_ID(), Strings.toChecksumHexString(address(iftOnA)), address(sendCallConstructor)
        );
    }

    function test_deployment() public view {
        IIFTMsgs.IFTBridge memory bridgeA = iftOnA.getIFTBridge(th.FIRST_CLIENT_ID());
        assertEq(bridgeA.clientId, th.FIRST_CLIENT_ID());
        assertEq(bridgeA.counterpartyIFTAddress, Strings.toChecksumHexString(address(iftOnB)));
        assertEq(address(bridgeA.iftSendCallConstructor), address(sendCallConstructor));

        IIFTMsgs.IFTBridge memory bridgeB = iftOnB.getIFTBridge(th.FIRST_CLIENT_ID());
        assertEq(bridgeB.clientId, th.FIRST_CLIENT_ID());
        assertEq(bridgeB.counterpartyIFTAddress, Strings.toChecksumHexString(address(iftOnA)));
        assertEq(address(bridgeB.iftSendCallConstructor), address(sendCallConstructor));
    }

    function testFuzz_success_iftTransferAcrossChains(uint256 amount) public {
        amount = bound(amount, 1, type(uint128).max - 1);

        address sender = integrationEnv.createUser();
        address receiver = integrationEnv.createUser();

        iftOnA.mint(sender, amount);
        assertEq(iftOnA.balanceOf(sender), amount);

        vm.startPrank(sender);
        vm.recordLogs();
        iftOnA.iftTransfer(th.FIRST_CLIENT_ID(), Strings.toHexString(receiver), amount);
        vm.stopPrank();

        assertEq(iftOnA.balanceOf(sender), 0, "tokens should be burned from sender");

        IICS26RouterMsgs.Packet memory sentPacket = _extractPacketFromLogs();

        IIFTMsgs.PendingTransfer memory pending = iftOnA.getPendingTransfer(th.FIRST_CLIENT_ID(), sentPacket.sequence);
        assertEq(pending.sender, sender);
        assertEq(pending.amount, amount);

        bytes[] memory acks = ibcImplB.recvPacket(sentPacket);
        assertEq(acks.length, 1, "should have one ack");

        assertEq(iftOnB.balanceOf(receiver), amount, "receiver should have received tokens");

        ibcImplA.ackPacket(sentPacket, acks);

        pending = iftOnA.getPendingTransfer(th.FIRST_CLIENT_ID(), sentPacket.sequence);
        assertEq(pending.sender, address(0), "pending transfer should be cleared");
        assertEq(pending.amount, 0, "pending amount should be zero");
    }

    function testFuzz_success_roundTripTransfer(uint256 amount) public {
        amount = bound(amount, 1, type(uint128).max - 1);

        address userA = integrationEnv.createUser();
        address userB = integrationEnv.createUser();

        iftOnA.mint(userA, amount);

        vm.startPrank(userA);
        vm.recordLogs();
        iftOnA.iftTransfer(th.FIRST_CLIENT_ID(), Strings.toHexString(userB), amount);
        vm.stopPrank();

        IICS26RouterMsgs.Packet memory packetAtoB = _extractPacketFromLogs();
        bytes[] memory acksAtoB = ibcImplB.recvPacket(packetAtoB);
        ibcImplA.ackPacket(packetAtoB, acksAtoB);

        assertEq(iftOnA.balanceOf(userA), 0);
        assertEq(iftOnB.balanceOf(userB), amount);

        vm.startPrank(userB);
        vm.recordLogs();
        iftOnB.iftTransfer(th.FIRST_CLIENT_ID(), Strings.toHexString(userA), amount);
        vm.stopPrank();

        IICS26RouterMsgs.Packet memory packetBtoA = _extractPacketFromLogs();
        bytes[] memory acksBtoA = ibcImplA.recvPacket(packetBtoA);
        ibcImplB.ackPacket(packetBtoA, acksBtoA);

        assertEq(iftOnB.balanceOf(userB), 0);
        assertEq(iftOnA.balanceOf(userA), amount);
    }

    function testFuzz_timeout_refundsTokens(uint256 amount) public {
        amount = bound(amount, 1, type(uint128).max - 1);

        address sender = integrationEnv.createUser();
        address receiver = integrationEnv.createUser();

        iftOnA.mint(sender, amount);

        uint64 shortTimeout = uint64(block.timestamp) + 1;

        vm.startPrank(sender);
        vm.recordLogs();
        iftOnA.iftTransfer(th.FIRST_CLIENT_ID(), Strings.toHexString(receiver), amount, shortTimeout);
        vm.stopPrank();

        IICS26RouterMsgs.Packet memory sentPacket = _extractPacketFromLogs();

        assertEq(iftOnA.balanceOf(sender), 0, "tokens should be burned");

        vm.warp(block.timestamp + 100);

        ibcImplA.cheatPacketCommitment(sentPacket);
        ibcImplA.timeoutPacket(sentPacket);

        assertEq(iftOnA.balanceOf(sender), amount, "tokens should be refunded");

        IIFTMsgs.PendingTransfer memory pending = iftOnA.getPendingTransfer(th.FIRST_CLIENT_ID(), sentPacket.sequence);
        assertEq(pending.sender, address(0), "pending transfer should be cleared");
        assertEq(pending.amount, 0);
    }

    function testFuzz_multipleTransfersInFlight(uint256 amount1, uint256 amount2) public {
        amount1 = bound(amount1, 1, type(uint64).max - 1);
        amount2 = bound(amount2, 1, type(uint64).max - 1);

        address sender = integrationEnv.createUser();
        address receiver1 = integrationEnv.createUser();
        address receiver2 = integrationEnv.createUser();

        iftOnA.mint(sender, amount1 + amount2);

        vm.startPrank(sender);
        vm.recordLogs();
        iftOnA.iftTransfer(th.FIRST_CLIENT_ID(), Strings.toHexString(receiver1), amount1);
        IICS26RouterMsgs.Packet memory packet1 = _extractPacketFromLogs();

        vm.recordLogs();
        iftOnA.iftTransfer(th.FIRST_CLIENT_ID(), Strings.toHexString(receiver2), amount2);
        IICS26RouterMsgs.Packet memory packet2 = _extractPacketFromLogs();
        vm.stopPrank();

        assertEq(packet2.sequence, packet1.sequence + 1, "sequences should be consecutive");

        bytes[] memory acks2 = ibcImplB.recvPacket(packet2);
        ibcImplA.ackPacket(packet2, acks2);
        assertEq(iftOnB.balanceOf(receiver2), amount2);

        bytes[] memory acks1 = ibcImplB.recvPacket(packet1);
        ibcImplA.ackPacket(packet1, acks1);
        assertEq(iftOnB.balanceOf(receiver1), amount1);
    }

    function _extractPacketFromLogs() internal returns (IICS26RouterMsgs.Packet memory) {
        bytes memory packetBz = th.getValueFromEvent(IICS26Router.SendPacket.selector);
        return abi.decode(packetBz, (IICS26RouterMsgs.Packet));
    }
}
