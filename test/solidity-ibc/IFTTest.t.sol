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
import { ERC1967Proxy } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Proxy.sol";

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

    string public constant COUNTERPARTY_IFT = "0x1234567890123456789012345678901234567890";

    uint256 public constant INITIAL_BALANCE = 1000 ether;

    string public constant TOKEN_NAME = "Test IFT";
    string public constant TOKEN_SYMBOL = "TIFT";

    function setUp() public {
        ics27Gmp = makeAddr("ics27Gmp");
        authority = makeAddr("authority");
        user1 = makeAddr("user1");
        user2 = makeAddr("user2");
        relayer = makeAddr("relayer");

        evmCallConstructor = new EVMIFTSendCallConstructor();

        IFTOwnable impl = new IFTOwnable();
        ERC1967Proxy proxy = new ERC1967Proxy(
            address(impl), abi.encodeCall(IFTOwnable.initialize, (authority, TOKEN_NAME, TOKEN_SYMBOL, ics27Gmp))
        );
        ift = IFTOwnable(address(proxy));

        // Give user1 some tokens
        deal(address(ift), user1, INITIAL_BALANCE, true);
    }

    // Helper Functions

    function _registerBridge() internal {
        string memory clientId = th.FIRST_CLIENT_ID();
        vm.prank(authority);
        ift.registerIFTBridge(clientId, COUNTERPARTY_IFT, address(evmCallConstructor));
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
            payload: IICS26RouterMsgs.Payload({
                sourcePort: ICS27Lib.DEFAULT_PORT_ID,
                destPort: ICS27Lib.DEFAULT_PORT_ID,
                version: ICS27Lib.ICS27_VERSION,
                encoding: ICS27Lib.ICS27_ENCODING,
                value: ""
            }),
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
            payload: IICS26RouterMsgs.Payload({
                sourcePort: ICS27Lib.DEFAULT_PORT_ID,
                destPort: ICS27Lib.DEFAULT_PORT_ID,
                version: ICS27Lib.ICS27_VERSION,
                encoding: ICS27Lib.ICS27_ENCODING,
                value: ""
            }),
            relayer: relayer
        });
    }

    // registerIFTBridge Tests

    function test_registerIFTBridge_success() public {
        string memory clientId = th.FIRST_CLIENT_ID();

        vm.expectEmit(true, true, true, true);
        emit IIFT.IFTBridgeRegistered(clientId, COUNTERPARTY_IFT, address(evmCallConstructor));

        vm.prank(authority);
        ift.registerIFTBridge(clientId, COUNTERPARTY_IFT, address(evmCallConstructor));

        IIFTMsgs.IFTBridge memory bridge = ift.getIFTBridge(clientId);
        assertEq(bridge.clientId, clientId);
        assertEq(bridge.counterpartyIFTAddress, COUNTERPARTY_IFT);
        assertEq(address(bridge.iftSendCallConstructor), address(evmCallConstructor));
    }

    function test_registerIFTBridge_overwrite() public {
        _registerBridge();

        string memory clientId = th.FIRST_CLIENT_ID();
        string memory newCounterparty = "0xabcdef1234567890abcdef1234567890abcdef12";
        EVMIFTSendCallConstructor newConstructor = new EVMIFTSendCallConstructor();

        vm.prank(authority);
        ift.registerIFTBridge(clientId, newCounterparty, address(newConstructor));

        IIFTMsgs.IFTBridge memory bridge = ift.getIFTBridge(clientId);
        assertEq(bridge.counterpartyIFTAddress, newCounterparty);
        assertEq(address(bridge.iftSendCallConstructor), address(newConstructor));
    }

    function test_registerIFTBridge_emptyClientId_reverts() public {
        vm.prank(authority);
        vm.expectRevert(IIFTErrors.IFTEmptyClientId.selector);
        ift.registerIFTBridge("", COUNTERPARTY_IFT, address(evmCallConstructor));
    }

    function test_registerIFTBridge_unauthorizedCaller_reverts() public {
        string memory clientId = th.FIRST_CLIENT_ID();
        vm.prank(user1);
        vm.expectRevert();
        ift.registerIFTBridge(clientId, COUNTERPARTY_IFT, address(evmCallConstructor));
    }

    function test_registerIFTBridge_zeroAddressConstructor_reverts() public {
        string memory clientId = th.FIRST_CLIENT_ID();
        vm.prank(authority);
        vm.expectRevert(IIFTErrors.IFTZeroAddressConstructor.selector);
        ift.registerIFTBridge(clientId, COUNTERPARTY_IFT, address(0));
    }

    function test_registerIFTBridge_emptyCounterpartyAddress_reverts() public {
        string memory clientId = th.FIRST_CLIENT_ID();
        vm.prank(authority);
        vm.expectRevert(IIFTErrors.IFTEmptyCounterpartyAddress.selector);
        ift.registerIFTBridge(clientId, "", address(evmCallConstructor));
    }

    // removeIFTBridge Tests

    function test_removeIFTBridge_success() public {
        _registerBridge();

        string memory clientId = th.FIRST_CLIENT_ID();

        vm.expectEmit(true, true, true, true);
        emit IIFT.IFTBridgeRemoved(clientId);

        vm.prank(authority);
        ift.removeIFTBridge(clientId);

        IIFTMsgs.IFTBridge memory bridge = ift.getIFTBridge(clientId);
        assertEq(bridge.clientId, "");
        assertEq(bridge.counterpartyIFTAddress, "");
    }

    function test_removeIFTBridge_unauthorizedCaller_reverts() public {
        _registerBridge();

        string memory clientId = th.FIRST_CLIENT_ID();
        vm.prank(user1);
        vm.expectRevert();
        ift.removeIFTBridge(clientId);
    }

    function test_removeIFTBridge_bridgeNotFound_reverts() public {
        string memory clientId = th.FIRST_CLIENT_ID();
        vm.prank(authority);
        vm.expectRevert(abi.encodeWithSelector(IIFTErrors.IFTBridgeNotFound.selector, clientId));
        ift.removeIFTBridge(clientId);
    }

    function testFuzz_removeIFTBridge_pendingTransfersStillProcessable(uint256 transferAmount) public {
        _registerBridge();

        transferAmount = bound(transferAmount, 1, INITIAL_BALANCE);
        string memory receiver = Strings.toHexString(user2);
        uint64 timeout = th.DEFAULT_TIMEOUT_TIMESTAMP();
        string memory clientId = th.FIRST_CLIENT_ID();

        _mockSendCall(clientId, COUNTERPARTY_IFT, receiver, transferAmount, timeout, 1);

        vm.prank(user1);
        ift.iftTransfer(clientId, receiver, transferAmount, timeout);

        IIFTMsgs.PendingTransfer memory pending = ift.getPendingTransfer(clientId, 1);
        assertEq(pending.sender, user1);
        assertEq(pending.amount, transferAmount);

        vm.prank(authority);
        ift.removeIFTBridge(clientId);

        pending = ift.getPendingTransfer(clientId, 1);
        assertEq(pending.sender, user1, "pending transfer should still exist after bridge removal");
        assertEq(pending.amount, transferAmount);

        IIBCAppCallbacks.OnTimeoutPacketCallback memory timeoutMsg = _createTimeoutCallback(clientId, 1);

        vm.prank(ics27Gmp);
        ift.onTimeoutPacket(timeoutMsg);

        assertEq(ift.balanceOf(user1), INITIAL_BALANCE, "tokens should be refunded");
    }

    function test_removeIFTBridge_cannotTransferAfterRemoval() public {
        _registerBridge();

        string memory clientId = th.FIRST_CLIENT_ID();

        vm.prank(authority);
        ift.removeIFTBridge(clientId);

        string memory receiver = Strings.toHexString(user2);
        uint64 timeout = th.DEFAULT_TIMEOUT_TIMESTAMP();

        vm.prank(user1);
        vm.expectRevert(abi.encodeWithSelector(IIFTErrors.IFTBridgeNotFound.selector, clientId));
        ift.iftTransfer(clientId, receiver, 100, timeout);
    }

    // iftTransfer Tests

    function testFuzz_iftTransfer_success(uint256 transferAmount) public {
        _registerBridge();

        transferAmount = bound(transferAmount, 1, INITIAL_BALANCE);
        string memory receiver = Strings.toHexString(user2);
        uint64 expectedSeq = 1;
        uint64 timeout = th.DEFAULT_TIMEOUT_TIMESTAMP();
        string memory clientId = th.FIRST_CLIENT_ID();

        _mockSendCall(clientId, COUNTERPARTY_IFT, receiver, transferAmount, timeout, expectedSeq);

        uint256 balanceBefore = ift.balanceOf(user1);

        vm.expectEmit(true, true, true, true);
        emit IIFT.IFTTransferInitiated(clientId, expectedSeq, user1, receiver, transferAmount);

        vm.prank(user1);
        ift.iftTransfer(clientId, receiver, transferAmount, timeout);

        assertEq(ift.balanceOf(user1), balanceBefore - transferAmount);

        IIFTMsgs.PendingTransfer memory pending = ift.getPendingTransfer(clientId, expectedSeq);
        assertEq(pending.sender, user1);
        assertEq(pending.amount, transferAmount);
    }

    function testFuzz_iftTransfer_multipleTransfers(uint256 amount1, uint256 amount2) public {
        _registerBridge();

        amount1 = bound(amount1, 1, INITIAL_BALANCE - 1);
        amount2 = bound(amount2, 1, INITIAL_BALANCE - amount1);
        string memory receiver = Strings.toHexString(user2);
        uint64 timeout = th.DEFAULT_TIMEOUT_TIMESTAMP();

        vm.startPrank(user1);
        _mockSendCall(th.FIRST_CLIENT_ID(), COUNTERPARTY_IFT, receiver, amount1, timeout, 1);
        ift.iftTransfer(th.FIRST_CLIENT_ID(), receiver, amount1, timeout);
        _mockSendCall(th.FIRST_CLIENT_ID(), COUNTERPARTY_IFT, receiver, amount2, timeout, 2);
        ift.iftTransfer(th.FIRST_CLIENT_ID(), receiver, amount2, timeout);
        vm.stopPrank();

        IIFTMsgs.PendingTransfer memory pending1 = ift.getPendingTransfer(th.FIRST_CLIENT_ID(), 1);
        IIFTMsgs.PendingTransfer memory pending2 = ift.getPendingTransfer(th.FIRST_CLIENT_ID(), 2);

        assertEq(pending1.amount, amount1);
        assertEq(pending2.amount, amount2);
    }

    function testFuzz_iftTransfer_emptyClientId_reverts(uint256 amount) public {
        _registerBridge();
        amount = bound(amount, 1, INITIAL_BALANCE);
        string memory receiver = Strings.toHexString(user2);
        uint64 timeout = th.DEFAULT_TIMEOUT_TIMESTAMP();

        vm.prank(user1);
        vm.expectRevert(IIFTErrors.IFTEmptyClientId.selector);
        ift.iftTransfer("", receiver, amount, timeout);
    }

    function testFuzz_iftTransfer_emptyReceiver_reverts(uint256 amount) public {
        _registerBridge();
        amount = bound(amount, 1, INITIAL_BALANCE);
        uint64 timeout = th.DEFAULT_TIMEOUT_TIMESTAMP();
        string memory clientId = th.FIRST_CLIENT_ID();

        vm.prank(user1);
        vm.expectRevert(IIFTErrors.IFTEmptyReceiver.selector);
        ift.iftTransfer(clientId, "", amount, timeout);
    }

    function test_iftTransfer_zeroAmount_reverts() public {
        _registerBridge();
        string memory receiver = Strings.toHexString(user2);
        uint64 timeout = th.DEFAULT_TIMEOUT_TIMESTAMP();
        string memory clientId = th.FIRST_CLIENT_ID();

        vm.prank(user1);
        vm.expectRevert(IIFTErrors.IFTZeroAmount.selector);
        ift.iftTransfer(clientId, receiver, 0, timeout);
    }

    function testFuzz_iftTransfer_bridgeNotFound_reverts(uint256 amount) public {
        amount = bound(amount, 1, INITIAL_BALANCE);
        string memory receiver = Strings.toHexString(user2);
        uint64 timeout = th.DEFAULT_TIMEOUT_TIMESTAMP();
        string memory invalidId = th.INVALID_ID();

        vm.prank(user1);
        vm.expectRevert(abi.encodeWithSelector(IIFTErrors.IFTBridgeNotFound.selector, invalidId));
        ift.iftTransfer(invalidId, receiver, amount, timeout);
    }

    function testFuzz_iftTransfer_insufficientBalance_reverts(uint256 amount) public {
        _registerBridge();
        string memory receiver = Strings.toHexString(user2);
        uint64 timeout = th.DEFAULT_TIMEOUT_TIMESTAMP();
        string memory clientId = th.FIRST_CLIENT_ID();
        amount = bound(amount, 1, type(uint256).max - INITIAL_BALANCE);
        amount += INITIAL_BALANCE;

        vm.prank(user1);
        vm.expectRevert();
        ift.iftTransfer(clientId, receiver, amount, timeout);
    }

    function testFuzz_iftTransfer_timeoutInPast_reverts(uint256 amount) public {
        _registerBridge();
        amount = bound(amount, 1, INITIAL_BALANCE);
        string memory receiver = Strings.toHexString(user2);
        uint64 pastTimeout = uint64(block.timestamp) - 1;
        string memory clientId = th.FIRST_CLIENT_ID();

        vm.prank(user1);
        vm.expectRevert(
            abi.encodeWithSelector(IIFTErrors.IFTTimeoutInPast.selector, pastTimeout, uint64(block.timestamp))
        );
        ift.iftTransfer(clientId, receiver, amount, pastTimeout);
    }

    function testFuzz_iftTransfer_timeoutAtCurrentTimestamp_reverts(uint256 amount) public {
        _registerBridge();
        amount = bound(amount, 1, INITIAL_BALANCE);
        string memory receiver = Strings.toHexString(user2);
        uint64 currentTimestamp = uint64(block.timestamp);
        string memory clientId = th.FIRST_CLIENT_ID();

        vm.prank(user1);
        vm.expectRevert(
            abi.encodeWithSelector(IIFTErrors.IFTTimeoutInPast.selector, currentTimestamp, uint64(block.timestamp))
        );
        ift.iftTransfer(clientId, receiver, amount, currentTimestamp);
    }

    function testFuzz_iftTransfer_defaultTimeout(uint256 transferAmount) public {
        _registerBridge();

        transferAmount = bound(transferAmount, 1, INITIAL_BALANCE);
        string memory receiver = Strings.toHexString(user2);
        uint64 expectedSeq = 1;
        string memory clientId = th.FIRST_CLIENT_ID();

        uint64 expectedTimeout = uint64(block.timestamp) + 15 minutes;
        _mockSendCall(clientId, COUNTERPARTY_IFT, receiver, transferAmount, expectedTimeout, expectedSeq);

        uint256 balanceBefore = ift.balanceOf(user1);

        vm.expectEmit(true, true, true, true);
        emit IIFT.IFTTransferInitiated(clientId, expectedSeq, user1, receiver, transferAmount);

        vm.prank(user1);
        ift.iftTransfer(clientId, receiver, transferAmount); // 3-param version with default timeout

        assertEq(ift.balanceOf(user1), balanceBefore - transferAmount);

        IIFTMsgs.PendingTransfer memory pending = ift.getPendingTransfer(clientId, expectedSeq);
        assertEq(pending.sender, user1);
        assertEq(pending.amount, transferAmount);
    }

    // iftMint Tests

    function testFuzz_iftMint_success(uint256 mintAmount) public {
        _registerBridge();

        address ics27Account = makeAddr("ics27Account");
        mintAmount = bound(mintAmount, 1, type(uint256).max - INITIAL_BALANCE);

        _mockAccountIdentifier(ics27Account, th.FIRST_CLIENT_ID(), COUNTERPARTY_IFT, "");

        uint256 balanceBefore = ift.balanceOf(user2);

        vm.expectEmit(true, true, true, true);
        emit IIFT.IFTMintReceived(th.FIRST_CLIENT_ID(), user2, mintAmount);

        vm.prank(ics27Account);
        ift.iftMint(user2, mintAmount);

        assertEq(ift.balanceOf(user2), balanceBefore + mintAmount);
    }

    function testFuzz_iftMint_bridgeNotFound_reverts(uint256 amount) public {
        address ics27Account = makeAddr("ics27Account");
        amount = bound(amount, 1, type(uint256).max);
        string memory invalidId = th.INVALID_ID();

        _mockAccountIdentifier(ics27Account, invalidId, COUNTERPARTY_IFT, "");

        vm.prank(ics27Account);
        vm.expectRevert(abi.encodeWithSelector(IIFTErrors.IFTBridgeNotFound.selector, invalidId));
        ift.iftMint(user2, amount);
    }

    function testFuzz_iftMint_unauthorizedSender_reverts(uint256 amount) public {
        _registerBridge();

        address ics27Account = makeAddr("ics27Account");
        string memory wrongSender = "0xwrongSenderAddress12345678901234567890";
        amount = bound(amount, 1, type(uint256).max);

        _mockAccountIdentifier(ics27Account, th.FIRST_CLIENT_ID(), wrongSender, "");

        vm.prank(ics27Account);
        vm.expectRevert(abi.encodeWithSelector(IIFTErrors.IFTUnauthorizedMint.selector, COUNTERPARTY_IFT, wrongSender));
        ift.iftMint(user2, amount);
    }

    function testFuzz_iftMint_unexpectedSalt_reverts(uint256 amount) public {
        _registerBridge();

        address ics27Account = makeAddr("ics27Account");
        bytes memory unexpectedSalt = hex"1234";
        amount = bound(amount, 1, type(uint256).max);

        _mockAccountIdentifier(ics27Account, th.FIRST_CLIENT_ID(), COUNTERPARTY_IFT, unexpectedSalt);

        vm.prank(ics27Account);
        vm.expectRevert(abi.encodeWithSelector(IIFTErrors.IFTUnexpectedSalt.selector, unexpectedSalt));
        ift.iftMint(user2, amount);
    }

    function testFuzz_iftMint_accountNotRegistered_reverts(uint256 amount) public {
        _registerBridge();
        amount = bound(amount, 1, type(uint256).max);

        address unknownAccount = makeAddr("unknownAccount");

        vm.prank(unknownAccount);
        vm.expectRevert();
        ift.iftMint(user2, amount);
    }

    function test_iftMint_zeroAmount() public {
        _registerBridge();

        address ics27Account = makeAddr("ics27Account");

        _mockAccountIdentifier(ics27Account, th.FIRST_CLIENT_ID(), COUNTERPARTY_IFT, "");

        // Zero amount mint is allowed by ERC20 (no-op)
        vm.prank(ics27Account);
        ift.iftMint(user2, 0);

        assertEq(ift.balanceOf(user2), 0);
    }

    function testFuzz_iftMint_toZeroAddress_reverts(uint256 amount) public {
        _registerBridge();

        address ics27Account = makeAddr("ics27Account");
        amount = bound(amount, 1, type(uint256).max);

        _mockAccountIdentifier(ics27Account, th.FIRST_CLIENT_ID(), COUNTERPARTY_IFT, "");

        // ERC20 reverts on mint to zero address
        vm.prank(ics27Account);
        vm.expectRevert();
        ift.iftMint(address(0), amount);
    }

    // onAckPacket Tests

    function testFuzz_onAckPacket_success_clearsTransfer(uint256 transferAmount) public {
        _registerBridge();

        transferAmount = bound(transferAmount, 1, INITIAL_BALANCE);
        string memory receiver = Strings.toHexString(user2);
        uint64 timeout = th.DEFAULT_TIMEOUT_TIMESTAMP();
        string memory clientId = th.FIRST_CLIENT_ID();

        _mockSendCall(clientId, COUNTERPARTY_IFT, receiver, transferAmount, timeout, 1);

        vm.prank(user1);
        ift.iftTransfer(clientId, receiver, transferAmount, timeout);

        uint64 seq = 1;
        bytes memory successAck = hex"01";

        IIBCAppCallbacks.OnAcknowledgementPacketCallback memory ackMsg =
            _createAckCallback(th.FIRST_CLIENT_ID(), seq, successAck);

        vm.expectEmit(true, true, true, true);
        emit IIFT.IFTTransferCompleted(th.FIRST_CLIENT_ID(), seq, user1, transferAmount);

        vm.prank(ics27Gmp);
        ift.onAckPacket(true, ackMsg);

        IIFTMsgs.PendingTransfer memory pending = ift.getPendingTransfer(th.FIRST_CLIENT_ID(), seq);
        assertEq(pending.amount, 0);
        assertEq(pending.sender, address(0));
    }

    function testFuzz_onAckPacket_failure_refundsTransfer(uint256 transferAmount) public {
        _registerBridge();

        transferAmount = bound(transferAmount, 1, INITIAL_BALANCE);
        string memory receiver = Strings.toHexString(user2);
        uint64 timeout = th.DEFAULT_TIMEOUT_TIMESTAMP();
        string memory clientId = th.FIRST_CLIENT_ID();

        _mockSendCall(clientId, COUNTERPARTY_IFT, receiver, transferAmount, timeout, 1);

        vm.prank(user1);
        ift.iftTransfer(clientId, receiver, transferAmount, timeout);

        uint256 balanceAfterTransfer = ift.balanceOf(user1);
        assertEq(balanceAfterTransfer, INITIAL_BALANCE - transferAmount);

        uint64 seq = 1;
        bytes memory errorAck = ICS24Host.UNIVERSAL_ERROR_ACK;

        IIBCAppCallbacks.OnAcknowledgementPacketCallback memory ackMsg =
            _createAckCallback(th.FIRST_CLIENT_ID(), seq, errorAck);

        vm.expectEmit(true, true, true, true);
        emit IIFT.IFTTransferRefunded(th.FIRST_CLIENT_ID(), seq, user1, transferAmount);

        vm.prank(ics27Gmp);
        ift.onAckPacket(false, ackMsg);

        assertEq(ift.balanceOf(user1), INITIAL_BALANCE);

        IIFTMsgs.PendingTransfer memory pending = ift.getPendingTransfer(th.FIRST_CLIENT_ID(), seq);
        assertEq(pending.amount, 0);
    }

    function test_onAckPacket_notICS27GMP_reverts() public {
        IIBCAppCallbacks.OnAcknowledgementPacketCallback memory ackMsg =
            _createAckCallback(th.FIRST_CLIENT_ID(), 1, hex"01");

        vm.prank(user1);
        vm.expectRevert(abi.encodeWithSelector(IIFTErrors.IFTOnlyICS27GMP.selector, user1));
        ift.onAckPacket(true, ackMsg);
    }

    function test_onAckPacket_noPendingTransfer_reverts() public {
        string memory clientId = th.FIRST_CLIENT_ID();
        IIBCAppCallbacks.OnAcknowledgementPacketCallback memory ackMsg =
            _createAckCallback(clientId, 999, ICS24Host.UNIVERSAL_ERROR_ACK);

        vm.prank(ics27Gmp);
        vm.expectRevert(abi.encodeWithSelector(IIFTErrors.IFTPendingTransferNotFound.selector, clientId, 999));
        ift.onAckPacket(false, ackMsg);
    }

    function testFuzz_onAckPacket_doubleAck_reverts(uint256 transferAmount) public {
        _registerBridge();

        transferAmount = bound(transferAmount, 1, INITIAL_BALANCE);
        string memory receiver = Strings.toHexString(user2);
        uint64 timeout = th.DEFAULT_TIMEOUT_TIMESTAMP();
        string memory clientId = th.FIRST_CLIENT_ID();

        _mockSendCall(clientId, COUNTERPARTY_IFT, receiver, transferAmount, timeout, 1);

        vm.prank(user1);
        ift.iftTransfer(clientId, receiver, transferAmount, timeout);

        uint64 seq = 1;
        bytes memory successAck = hex"01";

        IIBCAppCallbacks.OnAcknowledgementPacketCallback memory ackMsg = _createAckCallback(clientId, seq, successAck);

        // First ack succeeds
        vm.prank(ics27Gmp);
        ift.onAckPacket(true, ackMsg);

        // Second ack should fail (pending transfer already cleared)
        // With success=true, it just deletes and emits (no revert on empty)
        // With success=false, it tries to refund and reverts
        vm.prank(ics27Gmp);
        vm.expectRevert(abi.encodeWithSelector(IIFTErrors.IFTPendingTransferNotFound.selector, clientId, seq));
        ift.onAckPacket(false, ackMsg);
    }

    function test_onAckPacket_success_noPendingTransfer_reverts() public {
        // When success=true and no pending transfer exists, it should revert
        string memory clientId = th.FIRST_CLIENT_ID();
        IIBCAppCallbacks.OnAcknowledgementPacketCallback memory ackMsg = _createAckCallback(clientId, 999, hex"01");

        vm.prank(ics27Gmp);
        vm.expectRevert(abi.encodeWithSelector(IIFTErrors.IFTPendingTransferNotFound.selector, clientId, 999));
        ift.onAckPacket(true, ackMsg);
    }

    // onTimeoutPacket Tests

    function testFuzz_onTimeoutPacket_refundsTransfer(uint256 transferAmount) public {
        _registerBridge();

        transferAmount = bound(transferAmount, 1, INITIAL_BALANCE);
        string memory receiver = Strings.toHexString(user2);
        uint64 timeout = th.DEFAULT_TIMEOUT_TIMESTAMP();
        string memory clientId = th.FIRST_CLIENT_ID();

        _mockSendCall(clientId, COUNTERPARTY_IFT, receiver, transferAmount, timeout, 1);

        vm.prank(user1);
        ift.iftTransfer(clientId, receiver, transferAmount, timeout);

        uint256 balanceAfterTransfer = ift.balanceOf(user1);
        assertEq(balanceAfterTransfer, INITIAL_BALANCE - transferAmount);

        uint64 seq = 1;

        IIBCAppCallbacks.OnTimeoutPacketCallback memory timeoutMsg = _createTimeoutCallback(th.FIRST_CLIENT_ID(), seq);

        vm.expectEmit(true, true, true, true);
        emit IIFT.IFTTransferRefunded(th.FIRST_CLIENT_ID(), seq, user1, transferAmount);

        vm.prank(ics27Gmp);
        ift.onTimeoutPacket(timeoutMsg);

        assertEq(ift.balanceOf(user1), INITIAL_BALANCE);

        IIFTMsgs.PendingTransfer memory pending = ift.getPendingTransfer(th.FIRST_CLIENT_ID(), seq);
        assertEq(pending.amount, 0);
    }

    function test_onTimeoutPacket_notICS27GMP_reverts() public {
        IIBCAppCallbacks.OnTimeoutPacketCallback memory timeoutMsg = _createTimeoutCallback(th.FIRST_CLIENT_ID(), 1);

        vm.prank(user1);
        vm.expectRevert(abi.encodeWithSelector(IIFTErrors.IFTOnlyICS27GMP.selector, user1));
        ift.onTimeoutPacket(timeoutMsg);
    }

    function test_onTimeoutPacket_noPendingTransfer_reverts() public {
        string memory clientId = th.FIRST_CLIENT_ID();
        IIBCAppCallbacks.OnTimeoutPacketCallback memory timeoutMsg = _createTimeoutCallback(clientId, 999);

        vm.prank(ics27Gmp);
        vm.expectRevert(abi.encodeWithSelector(IIFTErrors.IFTPendingTransferNotFound.selector, clientId, 999));
        ift.onTimeoutPacket(timeoutMsg);
    }

    function testFuzz_onTimeoutPacket_doubleTimeout_reverts(uint256 transferAmount) public {
        _registerBridge();

        transferAmount = bound(transferAmount, 1, INITIAL_BALANCE);
        string memory receiver = Strings.toHexString(user2);
        uint64 timeout = th.DEFAULT_TIMEOUT_TIMESTAMP();
        string memory clientId = th.FIRST_CLIENT_ID();

        _mockSendCall(clientId, COUNTERPARTY_IFT, receiver, transferAmount, timeout, 1);

        vm.prank(user1);
        ift.iftTransfer(clientId, receiver, transferAmount, timeout);

        uint64 seq = 1;
        IIBCAppCallbacks.OnTimeoutPacketCallback memory timeoutMsg = _createTimeoutCallback(clientId, seq);

        // First timeout succeeds
        vm.prank(ics27Gmp);
        ift.onTimeoutPacket(timeoutMsg);

        // Second timeout should fail (pending transfer already cleared)
        vm.prank(ics27Gmp);
        vm.expectRevert(abi.encodeWithSelector(IIFTErrors.IFTPendingTransferNotFound.selector, clientId, seq));
        ift.onTimeoutPacket(timeoutMsg);
    }

    // View Functions Tests

    function test_getIFTBridge_notFound() public view {
        IIFTMsgs.IFTBridge memory bridge = ift.getIFTBridge(th.INVALID_ID());
        assertEq(bridge.clientId, "");
        assertEq(bridge.counterpartyIFTAddress, "");
    }

    function test_getPendingTransfer_notFound() public view {
        IIFTMsgs.PendingTransfer memory pending = ift.getPendingTransfer(th.FIRST_CLIENT_ID(), 999);
        assertEq(pending.sender, address(0));
        assertEq(pending.amount, 0);
    }

    function test_ics27() public view {
        assertEq(address(ift.ics27()), ics27Gmp);
    }

    // EVMIFTSendCallConstructor Tests

    function testFuzz_evmCallConstructor_constructMintCall(uint256 amount) public view {
        string memory receiver = Strings.toHexString(user2);
        amount = bound(amount, 0, type(uint256).max);

        bytes memory callData = evmCallConstructor.constructMintCall(receiver, amount);
        bytes memory expected = abi.encodeCall(IIFT.iftMint, (user2, amount));

        assertEq(callData, expected);
    }

    function testFuzz_evmCallConstructor_invalidReceiver_reverts(uint256 amount) public {
        amount = bound(amount, 0, type(uint256).max);
        vm.expectRevert();
        evmCallConstructor.constructMintCall("invalid-address", amount);
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

    function testFuzz_fullTransferCycle_success(uint256 transferAmount) public {
        _registerBridge();

        transferAmount = bound(transferAmount, 1, INITIAL_BALANCE);
        string memory receiver = Strings.toHexString(user2);
        uint64 timeout = th.DEFAULT_TIMEOUT_TIMESTAMP();
        string memory clientId = th.FIRST_CLIENT_ID();

        _mockSendCall(clientId, COUNTERPARTY_IFT, receiver, transferAmount, timeout, 1);

        // Step 1: User1 initiates transfer
        vm.prank(user1);
        ift.iftTransfer(clientId, receiver, transferAmount, timeout);

        assertEq(ift.balanceOf(user1), INITIAL_BALANCE - transferAmount);

        // Step 2: Simulate successful ack
        IIBCAppCallbacks.OnAcknowledgementPacketCallback memory ackMsg =
            _createAckCallback(th.FIRST_CLIENT_ID(), 1, hex"01");

        vm.prank(ics27Gmp);
        ift.onAckPacket(true, ackMsg);

        // Pending transfer should be cleared
        IIFTMsgs.PendingTransfer memory pending = ift.getPendingTransfer(th.FIRST_CLIENT_ID(), 1);
        assertEq(pending.amount, 0);
    }

    function testFuzz_fullTransferCycle_timeout(uint256 transferAmount) public {
        _registerBridge();

        transferAmount = bound(transferAmount, 1, INITIAL_BALANCE);
        string memory receiver = Strings.toHexString(user2);
        uint64 timeout = th.DEFAULT_TIMEOUT_TIMESTAMP();
        string memory clientId = th.FIRST_CLIENT_ID();

        _mockSendCall(clientId, COUNTERPARTY_IFT, receiver, transferAmount, timeout, 1);

        // Step 1: User1 initiates transfer
        vm.prank(user1);
        ift.iftTransfer(clientId, receiver, transferAmount, timeout);

        assertEq(ift.balanceOf(user1), INITIAL_BALANCE - transferAmount);

        // Step 2: Simulate timeout
        IIBCAppCallbacks.OnTimeoutPacketCallback memory timeoutMsg = _createTimeoutCallback(th.FIRST_CLIENT_ID(), 1);

        vm.prank(ics27Gmp);
        ift.onTimeoutPacket(timeoutMsg);

        // User should get refund
        assertEq(ift.balanceOf(user1), INITIAL_BALANCE);
    }

    function testFuzz_fullMintCycle(uint256 mintAmount) public {
        _registerBridge();

        mintAmount = bound(mintAmount, 1, type(uint256).max - INITIAL_BALANCE);
        address ics27Account = makeAddr("ics27Account");

        _mockAccountIdentifier(ics27Account, th.FIRST_CLIENT_ID(), COUNTERPARTY_IFT, "");

        // Simulate receiving mint from counterparty
        vm.prank(ics27Account);
        ift.iftMint(user2, mintAmount);

        assertEq(ift.balanceOf(user2), mintAmount);
    }

    function testFuzz_multipleBridges_independentTransfers(uint256 amount1, uint256 amount2) public {
        vm.assume(amount1 > 0 && amount2 > 0);

        string memory clientId2 = th.SECOND_CLIENT_ID();
        string memory counterparty2 = "0xabcdef1234567890abcdef1234567890abcdef12";
        EVMIFTSendCallConstructor constructor2 = new EVMIFTSendCallConstructor();

        // Register two bridges
        vm.startPrank(authority);
        ift.registerIFTBridge(th.FIRST_CLIENT_ID(), COUNTERPARTY_IFT, address(evmCallConstructor));
        ift.registerIFTBridge(clientId2, counterparty2, address(constructor2));
        vm.stopPrank();

        // Verify both bridges exist
        IIFTMsgs.IFTBridge memory bridge1 = ift.getIFTBridge(th.FIRST_CLIENT_ID());
        IIFTMsgs.IFTBridge memory bridge2 = ift.getIFTBridge(clientId2);

        assertEq(bridge1.counterpartyIFTAddress, COUNTERPARTY_IFT);
        assertEq(bridge2.counterpartyIFTAddress, counterparty2);

        amount1 = bound(amount1, 1, INITIAL_BALANCE - 1);
        amount2 = bound(amount2, 1, INITIAL_BALANCE - amount1);

        // Transfer on both bridges
        string memory receiver = Strings.toHexString(user2);
        uint64 timeout = th.DEFAULT_TIMEOUT_TIMESTAMP();

        vm.startPrank(user1);
        _mockSendCall(th.FIRST_CLIENT_ID(), COUNTERPARTY_IFT, receiver, amount1, timeout, 1);
        ift.iftTransfer(th.FIRST_CLIENT_ID(), receiver, amount1, timeout);
        _mockSendCall(clientId2, counterparty2, receiver, amount2, timeout, 2);
        ift.iftTransfer(clientId2, receiver, amount2, timeout);
        vm.stopPrank();

        // Both pending transfers should exist independently
        IIFTMsgs.PendingTransfer memory pending1 = ift.getPendingTransfer(th.FIRST_CLIENT_ID(), 1);
        IIFTMsgs.PendingTransfer memory pending2 = ift.getPendingTransfer(clientId2, 2);

        assertEq(pending1.amount, amount1);
        assertEq(pending2.amount, amount2);

        // Ack one, timeout the other
        vm.startPrank(ics27Gmp);
        ift.onAckPacket(true, _createAckCallback(th.FIRST_CLIENT_ID(), 1, hex"01"));
        ift.onTimeoutPacket(_createTimeoutCallback(clientId2, 2));
        vm.stopPrank();

        // First transfer completed, second refunded
        assertEq(ift.balanceOf(user1), INITIAL_BALANCE - amount1);
    }
}
