// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

// solhint-disable custom-errors,max-line-length,gas-small-strings,function-max-lines

import { Test } from "forge-std/Test.sol";

import { IIFTMsgs } from "../../contracts/msgs/IIFTMsgs.sol";
import { IICS27GMPMsgs } from "../../contracts/msgs/IICS27GMPMsgs.sol";
import { IIBCAppCallbacks } from "../../contracts/msgs/IIBCAppCallbacks.sol";
import { IICS26RouterMsgs } from "../../contracts/msgs/IICS26RouterMsgs.sol";

import { IIFT } from "../../contracts/interfaces/IIFT.sol";
import { IICS27GMP } from "../../contracts/interfaces/IICS27GMP.sol";
import { IIBCSenderCallbacks } from "../../contracts/interfaces/IIBCSenderCallbacks.sol";
import { IIFTErrors } from "../../contracts/errors/IIFTErrors.sol";

import { IFTOwnable } from "../../contracts/utils/IFTOwnable.sol";
import { EVMIFTSendCallConstructor } from "../../contracts/utils/EVMIFTSendCallConstructor.sol";
import { ICS24Host } from "../../contracts/utils/ICS24Host.sol";
import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";
import { TestHelper } from "./utils/TestHelper.sol";
import { ICS27Lib } from "../../contracts/utils/ICS27Lib.sol";

contract IFTTest is Test {
    // solhint-disable gas-indexed-events
    IFTOwnable public ift;
    EVMIFTSendCallConstructor public evmCallConstructor;

    TestHelper public th = new TestHelper();

    address public ics27Gmp;
    address public authority;
    address public user1;
    address public user2;
    address public relayer;

    string public constant CLIENT_ID = "client-0";
    string public constant COUNTERPARTY_IFT = "0x1234567890123456789012345678901234567890";

    uint256 public constant INITIAL_BALANCE = 1000 ether;

    bytes32 private constant IFT_STORAGE_SLOT =
        0x35d0029e62ce5824ad5e38215107659b8aa50b0046e8bc44a0f4a32b87d61a00;

    function setUp() public {
        ics27Gmp = makeAddr("ics27Gmp");
        authority = makeAddr("authority");
        user1 = makeAddr("user1");
        user2 = makeAddr("user2");
        relayer = makeAddr("relayer");

        evmCallConstructor = new EVMIFTSendCallConstructor();

        ift = new IFTOwnable();
        ift.initialize(authority);
        _setIcs27(address(ift), ics27Gmp);

        // Give user1 some tokens
        deal(address(ift), user1, INITIAL_BALANCE, true);
    }

    // Helper Functions

    function _registerBridge() internal {
        vm.prank(authority);
        ift.registerIFTBridge(CLIENT_ID, COUNTERPARTY_IFT, address(evmCallConstructor));
    }

    function _setIcs27(address token, address ics27) internal {
        // IFTAccessManaged/IFTOwnable do not expose an initializer for IFTBase storage.
        vm.store(token, IFT_STORAGE_SLOT, bytes32(uint256(uint160(ics27))));
    }

    function _mockSendCall(
        string memory clientId,
        string memory counterparty,
        string memory receiver,
        uint256 amount,
        uint64 timeoutTimestamp,
        uint64 seq
    )
        internal
    {
        bytes memory payload = evmCallConstructor.constructMintCall(receiver, amount);
        IICS27GMPMsgs.SendCallMsg memory msg_ = IICS27GMPMsgs.SendCallMsg({
            sourceClient: clientId,
            receiver: counterparty,
            salt: "",
            payload: payload,
            timeoutTimestamp: timeoutTimestamp,
            memo: ""
        });

        vm.mockCall(ics27Gmp, abi.encodeCall(IICS27GMP.sendCall, (msg_)), abi.encode(seq));
    }

    function _mockAccountIdentifier(
        address account,
        string memory clientId,
        string memory sender,
        bytes memory salt
    )
        internal
    {
        IICS27GMPMsgs.AccountIdentifier memory id =
            IICS27GMPMsgs.AccountIdentifier({ clientId: clientId, sender: sender, salt: salt });
        vm.mockCall(ics27Gmp, abi.encodeCall(IICS27GMP.getAccountIdentifier, (account)), abi.encode(id));
    }

    function _createAckCallback(
        string memory sourceClient,
        uint64 sequence,
        bytes memory ack
    )
        internal
        view
        returns (IIBCAppCallbacks.OnAcknowledgementPacketCallback memory)
    {
        return IIBCAppCallbacks.OnAcknowledgementPacketCallback({
            sourceClient: sourceClient,
            destinationClient: th.SECOND_CLIENT_ID(),
            sequence: sequence,
            payload: IICS26RouterMsgs.Payload({ sourcePort: ICS27Lib.DEFAULT_PORT_ID, destPort: ICS27Lib.DEFAULT_PORT_ID, version: ICS27Lib.ICS27_VERSION, encoding: ICS27Lib.ICS27_ENCODING, value: "" }),
            acknowledgement: ack,
            relayer: relayer
        });
    }

    function _createTimeoutCallback(
        string memory sourceClient,
        uint64 sequence
    )
        internal
        view
        returns (IIBCAppCallbacks.OnTimeoutPacketCallback memory)
    {
        return IIBCAppCallbacks.OnTimeoutPacketCallback({
            sourceClient: sourceClient,
            destinationClient: th.SECOND_CLIENT_ID(),
            sequence: sequence,
            payload: IICS26RouterMsgs.Payload({ sourcePort: ICS27Lib.DEFAULT_PORT_ID, destPort: ICS27Lib.DEFAULT_PORT_ID, version: ICS27Lib.ICS27_VERSION, encoding: ICS27Lib.ICS27_ENCODING, value: "" }),
            relayer: relayer
        });
    }

    // registerIFTBridge Tests

    function test_registerIFTBridge_success() public {
        vm.expectEmit(true, true, true, true);
        emit IIFT.IFTBridgeRegistered(CLIENT_ID, COUNTERPARTY_IFT, address(evmCallConstructor));

        vm.prank(authority);
        ift.registerIFTBridge(CLIENT_ID, COUNTERPARTY_IFT, address(evmCallConstructor));

        IIFTMsgs.IFTBridge memory bridge = ift.getIFTBridge(CLIENT_ID);
        assertEq(bridge.clientId, CLIENT_ID);
        assertEq(bridge.counterpartyIFTAddress, COUNTERPARTY_IFT);
        assertEq(address(bridge.iftSendCallConstructor), address(evmCallConstructor));
    }

    function test_registerIFTBridge_overwrite() public {
        _registerBridge();

        string memory newCounterparty = "0xabcdef1234567890abcdef1234567890abcdef12";
        EVMIFTSendCallConstructor newConstructor = new EVMIFTSendCallConstructor();

        vm.prank(authority);
        ift.registerIFTBridge(CLIENT_ID, newCounterparty, address(newConstructor));

        IIFTMsgs.IFTBridge memory bridge = ift.getIFTBridge(CLIENT_ID);
        assertEq(bridge.counterpartyIFTAddress, newCounterparty);
        assertEq(address(bridge.iftSendCallConstructor), address(newConstructor));
    }

    function test_registerIFTBridge_emptyClientId_reverts() public {
        vm.prank(authority);
        vm.expectRevert(IIFTErrors.IFTEmptyClientId.selector);
        ift.registerIFTBridge("", COUNTERPARTY_IFT, address(evmCallConstructor));
    }

    function test_registerIFTBridge_unauthorizedCaller_reverts() public {
        vm.prank(user1);
        vm.expectRevert();
        ift.registerIFTBridge(CLIENT_ID, COUNTERPARTY_IFT, address(evmCallConstructor));
    }

    function test_registerIFTBridge_zeroAddressConstructor_reverts() public {
        vm.prank(authority);
        vm.expectRevert(IIFTErrors.IFTZeroAddressConstructor.selector);
        ift.registerIFTBridge(CLIENT_ID, COUNTERPARTY_IFT, address(0));
    }

    function test_registerIFTBridge_emptyCounterpartyAddress_reverts() public {
        vm.prank(authority);
        vm.expectRevert(IIFTErrors.IFTEmptyCounterpartyAddress.selector);
        ift.registerIFTBridge(CLIENT_ID, "", address(evmCallConstructor));
    }

    // iftTransfer Tests

    function test_iftTransfer_success() public {
        _registerBridge();

        uint256 transferAmount = 100 ether;
        string memory receiver = Strings.toHexString(user2);
        uint64 expectedSeq = 1;
        uint64 timeout = th.DEFAULT_TIMEOUT_TIMESTAMP();

        _mockSendCall(CLIENT_ID, COUNTERPARTY_IFT, receiver, transferAmount, timeout, expectedSeq);

        uint256 balanceBefore = ift.balanceOf(user1);

        vm.expectEmit(true, true, true, true);
        emit IIFT.IFTTransferInitiated(CLIENT_ID, expectedSeq, user1, receiver, transferAmount);

        vm.prank(user1);
        ift.iftTransfer(CLIENT_ID, receiver, transferAmount, timeout);

        assertEq(ift.balanceOf(user1), balanceBefore - transferAmount);

        IIFTMsgs.PendingTransfer memory pending = ift.getPendingTransfer(CLIENT_ID, expectedSeq);
        assertEq(pending.sender, user1);
        assertEq(pending.amount, transferAmount);
    }

    function test_iftTransfer_multipleTransfers() public {
        _registerBridge();

        uint256 amount1 = 50 ether;
        uint256 amount2 = 30 ether;
        string memory receiver = Strings.toHexString(user2);
        uint64 timeout = th.DEFAULT_TIMEOUT_TIMESTAMP();

        vm.startPrank(user1);
        _mockSendCall(CLIENT_ID, COUNTERPARTY_IFT, receiver, amount1, timeout, 1);
        ift.iftTransfer(CLIENT_ID, receiver, amount1, timeout);
        _mockSendCall(CLIENT_ID, COUNTERPARTY_IFT, receiver, amount2, timeout, 2);
        ift.iftTransfer(CLIENT_ID, receiver, amount2, timeout);
        vm.stopPrank();

        IIFTMsgs.PendingTransfer memory pending1 = ift.getPendingTransfer(CLIENT_ID, 1);
        IIFTMsgs.PendingTransfer memory pending2 = ift.getPendingTransfer(CLIENT_ID, 2);

        assertEq(pending1.amount, amount1);
        assertEq(pending2.amount, amount2);
    }

    function test_iftTransfer_emptyClientId_reverts() public {
        _registerBridge();
        string memory receiver = Strings.toHexString(user2);
        uint64 timeout = th.DEFAULT_TIMEOUT_TIMESTAMP();

        vm.prank(user1);
        vm.expectRevert(IIFTErrors.IFTEmptyClientId.selector);
        ift.iftTransfer("", receiver, 100 ether, timeout);
    }

    function test_iftTransfer_emptyReceiver_reverts() public {
        _registerBridge();
        uint64 timeout = th.DEFAULT_TIMEOUT_TIMESTAMP();

        vm.prank(user1);
        vm.expectRevert(IIFTErrors.IFTEmptyReceiver.selector);
        ift.iftTransfer(CLIENT_ID, "", 100 ether, timeout);
    }

    function test_iftTransfer_zeroAmount_reverts() public {
        _registerBridge();
        string memory receiver = Strings.toHexString(user2);
        uint64 timeout = th.DEFAULT_TIMEOUT_TIMESTAMP();

        vm.prank(user1);
        vm.expectRevert(IIFTErrors.IFTZeroAmount.selector);
        ift.iftTransfer(CLIENT_ID, receiver, 0, timeout);
    }

    function test_iftTransfer_bridgeNotFound_reverts() public {
        string memory receiver = Strings.toHexString(user2);
        uint64 timeout = th.DEFAULT_TIMEOUT_TIMESTAMP();

        vm.prank(user1);
        vm.expectRevert(abi.encodeWithSelector(IIFTErrors.IFTBridgeNotFound.selector, "unknown-client"));
        ift.iftTransfer("unknown-client", receiver, 100 ether, timeout);
    }

    function test_iftTransfer_insufficientBalance_reverts() public {
        _registerBridge();
        string memory receiver = Strings.toHexString(user2);
        uint64 timeout = th.DEFAULT_TIMEOUT_TIMESTAMP();

        vm.prank(user1);
        vm.expectRevert();
        ift.iftTransfer(CLIENT_ID, receiver, INITIAL_BALANCE + 1, timeout);
    }

    function test_iftTransfer_timeoutInPast_reverts() public {
        _registerBridge();
        string memory receiver = Strings.toHexString(user2);
        uint64 pastTimeout = uint64(block.timestamp) - 1;

        vm.prank(user1);
        vm.expectRevert(
            abi.encodeWithSelector(IIFTErrors.IFTTimeoutInPast.selector, pastTimeout, uint64(block.timestamp))
        );
        ift.iftTransfer(CLIENT_ID, receiver, 100 ether, pastTimeout);
    }

    function test_iftTransfer_timeoutAtCurrentTimestamp_reverts() public {
        _registerBridge();
        string memory receiver = Strings.toHexString(user2);
        uint64 currentTimestamp = uint64(block.timestamp);

        vm.prank(user1);
        vm.expectRevert(
            abi.encodeWithSelector(IIFTErrors.IFTTimeoutInPast.selector, currentTimestamp, uint64(block.timestamp))
        );
        ift.iftTransfer(CLIENT_ID, receiver, 100 ether, currentTimestamp);
    }

    function test_iftTransfer_defaultTimeout() public {
        _registerBridge();

        uint256 transferAmount = 100 ether;
        string memory receiver = Strings.toHexString(user2);
        uint64 expectedSeq = 1;

        uint64 expectedTimeout = uint64(block.timestamp) + 15 minutes;
        _mockSendCall(CLIENT_ID, COUNTERPARTY_IFT, receiver, transferAmount, expectedTimeout, expectedSeq);

        uint256 balanceBefore = ift.balanceOf(user1);

        vm.expectEmit(true, true, true, true);
        emit IIFT.IFTTransferInitiated(CLIENT_ID, expectedSeq, user1, receiver, transferAmount);

        vm.prank(user1);
        ift.iftTransfer(CLIENT_ID, receiver, transferAmount); // 3-param version with default timeout

        assertEq(ift.balanceOf(user1), balanceBefore - transferAmount);

        IIFTMsgs.PendingTransfer memory pending = ift.getPendingTransfer(CLIENT_ID, expectedSeq);
        assertEq(pending.sender, user1);
        assertEq(pending.amount, transferAmount);
    }

    function testFuzz_iftTransfer_success(uint256 amount) public {
        amount = bound(amount, 1, INITIAL_BALANCE);
        _registerBridge();

        string memory receiver = Strings.toHexString(user2);
        uint64 timeout = th.DEFAULT_TIMEOUT_TIMESTAMP();

        _mockSendCall(CLIENT_ID, COUNTERPARTY_IFT, receiver, amount, timeout, 1);

        vm.prank(user1);
        ift.iftTransfer(CLIENT_ID, receiver, amount, timeout);

        assertEq(ift.balanceOf(user1), INITIAL_BALANCE - amount);
    }

    // iftMint Tests

    function test_iftMint_success() public {
        _registerBridge();

        address ics27Account = makeAddr("ics27Account");
        uint256 mintAmount = 200 ether;

        _mockAccountIdentifier(ics27Account, CLIENT_ID, COUNTERPARTY_IFT, "");

        uint256 balanceBefore = ift.balanceOf(user2);

        vm.expectEmit(true, true, true, true);
        emit IIFT.IFTMintReceived(CLIENT_ID, user2, mintAmount);

        vm.prank(ics27Account);
        ift.iftMint(user2, mintAmount);

        assertEq(ift.balanceOf(user2), balanceBefore + mintAmount);
    }

    function test_iftMint_bridgeNotFound_reverts() public {
        address ics27Account = makeAddr("ics27Account");

        _mockAccountIdentifier(ics27Account, "unknown-client", COUNTERPARTY_IFT, "");

        vm.prank(ics27Account);
        vm.expectRevert(abi.encodeWithSelector(IIFTErrors.IFTBridgeNotFound.selector, "unknown-client"));
        ift.iftMint(user2, 100 ether);
    }

    function test_iftMint_unauthorizedSender_reverts() public {
        _registerBridge();

        address ics27Account = makeAddr("ics27Account");
        string memory wrongSender = "0xwrongSenderAddress12345678901234567890";

        _mockAccountIdentifier(ics27Account, CLIENT_ID, wrongSender, "");

        vm.prank(ics27Account);
        vm.expectRevert(abi.encodeWithSelector(IIFTErrors.IFTUnauthorizedMint.selector, COUNTERPARTY_IFT, wrongSender));
        ift.iftMint(user2, 100 ether);
    }

    function test_iftMint_unexpectedSalt_reverts() public {
        _registerBridge();

        address ics27Account = makeAddr("ics27Account");
        bytes memory unexpectedSalt = hex"1234";

        _mockAccountIdentifier(ics27Account, CLIENT_ID, COUNTERPARTY_IFT, unexpectedSalt);

        vm.prank(ics27Account);
        vm.expectRevert(abi.encodeWithSelector(IIFTErrors.IFTUnexpectedSalt.selector, unexpectedSalt));
        ift.iftMint(user2, 100 ether);
    }

    function test_iftMint_accountNotRegistered_reverts() public {
        _registerBridge();

        address unknownAccount = makeAddr("unknownAccount");

        vm.prank(unknownAccount);
        vm.expectRevert();
        ift.iftMint(user2, 100 ether);
    }

    function testFuzz_iftMint_success(uint256 amount) public {
        vm.assume(amount > 0 && amount < type(uint256).max - INITIAL_BALANCE);
        _registerBridge();

        address ics27Account = makeAddr("ics27Account");

        _mockAccountIdentifier(ics27Account, CLIENT_ID, COUNTERPARTY_IFT, "");

        vm.prank(ics27Account);
        ift.iftMint(user2, amount);

        assertEq(ift.balanceOf(user2), amount);
    }

    function test_iftMint_zeroAmount() public {
        _registerBridge();

        address ics27Account = makeAddr("ics27Account");

        _mockAccountIdentifier(ics27Account, CLIENT_ID, COUNTERPARTY_IFT, "");

        // Zero amount mint is allowed by ERC20 (no-op)
        vm.prank(ics27Account);
        ift.iftMint(user2, 0);

        assertEq(ift.balanceOf(user2), 0);
    }

    function test_iftMint_toZeroAddress_reverts() public {
        _registerBridge();

        address ics27Account = makeAddr("ics27Account");

        _mockAccountIdentifier(ics27Account, CLIENT_ID, COUNTERPARTY_IFT, "");

        // ERC20 reverts on mint to zero address
        vm.prank(ics27Account);
        vm.expectRevert();
        ift.iftMint(address(0), 100 ether);
    }

    // onAckPacket Tests

    function test_onAckPacket_success_clearsTransfer() public {
        _registerBridge();

        uint256 transferAmount = 100 ether;
        string memory receiver = Strings.toHexString(user2);
        uint64 timeout = th.DEFAULT_TIMEOUT_TIMESTAMP();

        _mockSendCall(CLIENT_ID, COUNTERPARTY_IFT, receiver, transferAmount, timeout, 1);

        vm.prank(user1);
        ift.iftTransfer(CLIENT_ID, receiver, transferAmount, timeout);

        uint64 seq = 1;
        bytes memory successAck = hex"01";

        IIBCAppCallbacks.OnAcknowledgementPacketCallback memory ackMsg = _createAckCallback(CLIENT_ID, seq, successAck);

        vm.expectEmit(true, true, true, true);
        emit IIFT.IFTTransferCompleted(CLIENT_ID, seq, user1, transferAmount);

        vm.prank(ics27Gmp);
        ift.onAckPacket(true, ackMsg);

        IIFTMsgs.PendingTransfer memory pending = ift.getPendingTransfer(CLIENT_ID, seq);
        assertEq(pending.amount, 0);
        assertEq(pending.sender, address(0));
    }

    function test_onAckPacket_failure_refundsTransfer() public {
        _registerBridge();

        uint256 transferAmount = 100 ether;
        string memory receiver = Strings.toHexString(user2);
        uint64 timeout = th.DEFAULT_TIMEOUT_TIMESTAMP();

        _mockSendCall(CLIENT_ID, COUNTERPARTY_IFT, receiver, transferAmount, timeout, 1);

        vm.prank(user1);
        ift.iftTransfer(CLIENT_ID, receiver, transferAmount, timeout);

        uint256 balanceAfterTransfer = ift.balanceOf(user1);
        assertEq(balanceAfterTransfer, INITIAL_BALANCE - transferAmount);

        uint64 seq = 1;
        bytes memory errorAck = ICS24Host.UNIVERSAL_ERROR_ACK;

        IIBCAppCallbacks.OnAcknowledgementPacketCallback memory ackMsg = _createAckCallback(CLIENT_ID, seq, errorAck);

        vm.expectEmit(true, true, true, true);
        emit IIFT.IFTTransferRefunded(CLIENT_ID, seq, user1, transferAmount);

        vm.prank(ics27Gmp);
        ift.onAckPacket(false, ackMsg);

        assertEq(ift.balanceOf(user1), INITIAL_BALANCE);

        IIFTMsgs.PendingTransfer memory pending = ift.getPendingTransfer(CLIENT_ID, seq);
        assertEq(pending.amount, 0);
    }

    function test_onAckPacket_notICS27GMP_reverts() public {
        IIBCAppCallbacks.OnAcknowledgementPacketCallback memory ackMsg = _createAckCallback(CLIENT_ID, 1, hex"01");

        vm.prank(user1);
        vm.expectRevert(abi.encodeWithSelector(IIFTErrors.IFTOnlyICS27GMP.selector, user1));
        ift.onAckPacket(true, ackMsg);
    }

    function test_onAckPacket_noPendingTransfer_reverts() public {
        IIBCAppCallbacks.OnAcknowledgementPacketCallback memory ackMsg =
            _createAckCallback(CLIENT_ID, 999, ICS24Host.UNIVERSAL_ERROR_ACK);

        vm.prank(ics27Gmp);
        vm.expectRevert(abi.encodeWithSelector(IIFTErrors.IFTPendingTransferNotFound.selector, CLIENT_ID, 999));
        ift.onAckPacket(false, ackMsg);
    }

    function test_onAckPacket_doubleAck_reverts() public {
        _registerBridge();

        uint256 transferAmount = 100 ether;
        string memory receiver = Strings.toHexString(user2);
        uint64 timeout = th.DEFAULT_TIMEOUT_TIMESTAMP();

        _mockSendCall(CLIENT_ID, COUNTERPARTY_IFT, receiver, transferAmount, timeout, 1);

        vm.prank(user1);
        ift.iftTransfer(CLIENT_ID, receiver, transferAmount, timeout);

        uint64 seq = 1;
        bytes memory successAck = hex"01";

        IIBCAppCallbacks.OnAcknowledgementPacketCallback memory ackMsg = _createAckCallback(CLIENT_ID, seq, successAck);

        // First ack succeeds
        vm.prank(ics27Gmp);
        ift.onAckPacket(true, ackMsg);

        // Second ack should fail (pending transfer already cleared)
        // With success=true, it just deletes and emits (no revert on empty)
        // With success=false, it tries to refund and reverts
        vm.prank(ics27Gmp);
        vm.expectRevert(abi.encodeWithSelector(IIFTErrors.IFTPendingTransferNotFound.selector, CLIENT_ID, seq));
        ift.onAckPacket(false, ackMsg);
    }

    function test_onAckPacket_success_noPendingTransfer_reverts() public {
        // When success=true and no pending transfer exists, it should revert
        IIBCAppCallbacks.OnAcknowledgementPacketCallback memory ackMsg = _createAckCallback(CLIENT_ID, 999, hex"01");

        vm.prank(ics27Gmp);
        vm.expectRevert(abi.encodeWithSelector(IIFTErrors.IFTPendingTransferNotFound.selector, CLIENT_ID, 999));
        ift.onAckPacket(true, ackMsg);
    }

    // onTimeoutPacket Tests

    function test_onTimeoutPacket_refundsTransfer() public {
        _registerBridge();

        uint256 transferAmount = 100 ether;
        string memory receiver = Strings.toHexString(user2);
        uint64 timeout = th.DEFAULT_TIMEOUT_TIMESTAMP();

        _mockSendCall(CLIENT_ID, COUNTERPARTY_IFT, receiver, transferAmount, timeout, 1);

        vm.prank(user1);
        ift.iftTransfer(CLIENT_ID, receiver, transferAmount, timeout);

        uint256 balanceAfterTransfer = ift.balanceOf(user1);
        assertEq(balanceAfterTransfer, INITIAL_BALANCE - transferAmount);

        uint64 seq = 1;

        IIBCAppCallbacks.OnTimeoutPacketCallback memory timeoutMsg = _createTimeoutCallback(CLIENT_ID, seq);

        vm.expectEmit(true, true, true, true);
        emit IIFT.IFTTransferRefunded(CLIENT_ID, seq, user1, transferAmount);

        vm.prank(ics27Gmp);
        ift.onTimeoutPacket(timeoutMsg);

        assertEq(ift.balanceOf(user1), INITIAL_BALANCE);

        IIFTMsgs.PendingTransfer memory pending = ift.getPendingTransfer(CLIENT_ID, seq);
        assertEq(pending.amount, 0);
    }

    function test_onTimeoutPacket_notICS27GMP_reverts() public {
        IIBCAppCallbacks.OnTimeoutPacketCallback memory timeoutMsg = _createTimeoutCallback(CLIENT_ID, 1);

        vm.prank(user1);
        vm.expectRevert(abi.encodeWithSelector(IIFTErrors.IFTOnlyICS27GMP.selector, user1));
        ift.onTimeoutPacket(timeoutMsg);
    }

    function test_onTimeoutPacket_noPendingTransfer_reverts() public {
        IIBCAppCallbacks.OnTimeoutPacketCallback memory timeoutMsg = _createTimeoutCallback(CLIENT_ID, 999);

        vm.prank(ics27Gmp);
        vm.expectRevert(abi.encodeWithSelector(IIFTErrors.IFTPendingTransferNotFound.selector, CLIENT_ID, 999));
        ift.onTimeoutPacket(timeoutMsg);
    }

    function test_onTimeoutPacket_doubleTimeout_reverts() public {
        _registerBridge();

        uint256 transferAmount = 100 ether;
        string memory receiver = Strings.toHexString(user2);
        uint64 timeout = th.DEFAULT_TIMEOUT_TIMESTAMP();

        _mockSendCall(CLIENT_ID, COUNTERPARTY_IFT, receiver, transferAmount, timeout, 1);

        vm.prank(user1);
        ift.iftTransfer(CLIENT_ID, receiver, transferAmount, timeout);

        uint64 seq = 1;
        IIBCAppCallbacks.OnTimeoutPacketCallback memory timeoutMsg = _createTimeoutCallback(CLIENT_ID, seq);

        // First timeout succeeds
        vm.prank(ics27Gmp);
        ift.onTimeoutPacket(timeoutMsg);

        // Second timeout should fail (pending transfer already cleared)
        vm.prank(ics27Gmp);
        vm.expectRevert(abi.encodeWithSelector(IIFTErrors.IFTPendingTransferNotFound.selector, CLIENT_ID, seq));
        ift.onTimeoutPacket(timeoutMsg);
    }

    // View Functions Tests

    function test_getIFTBridge_notFound() public view {
        IIFTMsgs.IFTBridge memory bridge = ift.getIFTBridge("nonexistent");
        assertEq(bridge.clientId, "");
        assertEq(bridge.counterpartyIFTAddress, "");
    }

    function test_getPendingTransfer_notFound() public view {
        IIFTMsgs.PendingTransfer memory pending = ift.getPendingTransfer(CLIENT_ID, 999);
        assertEq(pending.sender, address(0));
        assertEq(pending.amount, 0);
    }

    function test_ics27() public view {
        assertEq(address(ift.ics27()), ics27Gmp);
    }

    // EVMIFTSendCallConstructor Tests

    function test_evmCallConstructor_constructMintCall() public view {
        string memory receiver = Strings.toHexString(user2);
        uint256 amount = 500 ether;

        bytes memory callData = evmCallConstructor.constructMintCall(receiver, amount);
        bytes memory expected = abi.encodeCall(IIFT.iftMint, (user2, amount));

        assertEq(callData, expected);
    }

    function test_evmCallConstructor_invalidReceiver_reverts() public {
        vm.expectRevert();
        evmCallConstructor.constructMintCall("invalid-address", 100 ether);
    }

    // ERC165 Interface Tests

    function test_supportsInterface() public view {
        // IIBCSenderCallbacks interface ID
        bytes4 senderCallbacksId = type(IIBCSenderCallbacks).interfaceId;
        assertTrue(ift.supportsInterface(senderCallbacksId));

        // ERC165 interface ID
        bytes4 erc165Id = 0x01ffc9a7;
        assertTrue(ift.supportsInterface(erc165Id));
    }

    // Integration Scenario Tests

    function test_fullTransferCycle_success() public {
        _registerBridge();

        uint256 transferAmount = 100 ether;
        string memory receiver = Strings.toHexString(user2);
        uint64 timeout = th.DEFAULT_TIMEOUT_TIMESTAMP();

        _mockSendCall(CLIENT_ID, COUNTERPARTY_IFT, receiver, transferAmount, timeout, 1);

        // Step 1: User1 initiates transfer
        vm.prank(user1);
        ift.iftTransfer(CLIENT_ID, receiver, transferAmount, timeout);

        assertEq(ift.balanceOf(user1), INITIAL_BALANCE - transferAmount);

        // Step 2: Simulate successful ack
        IIBCAppCallbacks.OnAcknowledgementPacketCallback memory ackMsg = _createAckCallback(CLIENT_ID, 1, hex"01");

        vm.prank(ics27Gmp);
        ift.onAckPacket(true, ackMsg);

        // Pending transfer should be cleared
        IIFTMsgs.PendingTransfer memory pending = ift.getPendingTransfer(CLIENT_ID, 1);
        assertEq(pending.amount, 0);
    }

    function test_fullTransferCycle_timeout() public {
        _registerBridge();

        uint256 transferAmount = 100 ether;
        string memory receiver = Strings.toHexString(user2);
        uint64 timeout = th.DEFAULT_TIMEOUT_TIMESTAMP();

        _mockSendCall(CLIENT_ID, COUNTERPARTY_IFT, receiver, transferAmount, timeout, 1);

        // Step 1: User1 initiates transfer
        vm.prank(user1);
        ift.iftTransfer(CLIENT_ID, receiver, transferAmount, timeout);

        assertEq(ift.balanceOf(user1), INITIAL_BALANCE - transferAmount);

        // Step 2: Simulate timeout
        IIBCAppCallbacks.OnTimeoutPacketCallback memory timeoutMsg = _createTimeoutCallback(CLIENT_ID, 1);

        vm.prank(ics27Gmp);
        ift.onTimeoutPacket(timeoutMsg);

        // User should get refund
        assertEq(ift.balanceOf(user1), INITIAL_BALANCE);
    }

    function test_fullMintCycle() public {
        _registerBridge();

        uint256 mintAmount = 500 ether;
        address ics27Account = makeAddr("ics27Account");

        _mockAccountIdentifier(ics27Account, CLIENT_ID, COUNTERPARTY_IFT, "");

        // Simulate receiving mint from counterparty
        vm.prank(ics27Account);
        ift.iftMint(user2, mintAmount);

        assertEq(ift.balanceOf(user2), mintAmount);
    }

    function test_multipleBridges_independentTransfers() public {
        string memory clientId2 = "client-1";
        string memory counterparty2 = "0xabcdef1234567890abcdef1234567890abcdef12";
        EVMIFTSendCallConstructor constructor2 = new EVMIFTSendCallConstructor();

        // Register two bridges
        vm.startPrank(authority);
        ift.registerIFTBridge(CLIENT_ID, COUNTERPARTY_IFT, address(evmCallConstructor));
        ift.registerIFTBridge(clientId2, counterparty2, address(constructor2));
        vm.stopPrank();

        // Verify both bridges exist
        IIFTMsgs.IFTBridge memory bridge1 = ift.getIFTBridge(CLIENT_ID);
        IIFTMsgs.IFTBridge memory bridge2 = ift.getIFTBridge(clientId2);

        assertEq(bridge1.counterpartyIFTAddress, COUNTERPARTY_IFT);
        assertEq(bridge2.counterpartyIFTAddress, counterparty2);

        // Transfer on both bridges
        string memory receiver = Strings.toHexString(user2);
        uint64 timeout = th.DEFAULT_TIMEOUT_TIMESTAMP();

        vm.startPrank(user1);
        _mockSendCall(CLIENT_ID, COUNTERPARTY_IFT, receiver, 100 ether, timeout, 1);
        ift.iftTransfer(CLIENT_ID, receiver, 100 ether, timeout);
        _mockSendCall(clientId2, counterparty2, receiver, 200 ether, timeout, 2);
        ift.iftTransfer(clientId2, receiver, 200 ether, timeout);
        vm.stopPrank();

        // Both pending transfers should exist independently
        IIFTMsgs.PendingTransfer memory pending1 = ift.getPendingTransfer(CLIENT_ID, 1);
        IIFTMsgs.PendingTransfer memory pending2 = ift.getPendingTransfer(clientId2, 2);

        assertEq(pending1.amount, 100 ether);
        assertEq(pending2.amount, 200 ether);

        // Ack one, timeout the other
        vm.startPrank(ics27Gmp);
        ift.onAckPacket(true, _createAckCallback(CLIENT_ID, 1, hex"01"));
        ift.onTimeoutPacket(_createTimeoutCallback(clientId2, 2));
        vm.stopPrank();

        // First transfer completed, second refunded
        assertEq(ift.balanceOf(user1), INITIAL_BALANCE - 100 ether);
    }
}
