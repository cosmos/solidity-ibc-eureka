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

import { IFTBase } from "../../contracts/IFTBase.sol";
import { EVMIFTSendCallConstructor } from "../../contracts/utils/EVMIFTSendCallConstructor.sol";
import { ICS24Host } from "../../contracts/utils/ICS24Host.sol";

import { ERC20 } from "@openzeppelin-contracts/token/ERC20/ERC20.sol";
import { AccessManager } from "@openzeppelin-contracts/access/manager/AccessManager.sol";
import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";

import { TestHelper } from "./utils/TestHelper.sol";

/// @title Test IFT Token
/// @notice A concrete implementation of IFTBase for testing purposes
contract TestIFT is IFTBase {
    constructor(
        IICS27GMP ics27Gmp_,
        address authority_
    )
        ERC20("Mock Interchain Token", "MIFT")
        IFTBase(ics27Gmp_, authority_)
    { }

    /// @notice Mint tokens for testing
    function mint(address to, uint256 amount) external {
        _mint(to, amount);
    }
}

/// @title Mock ICS27GMP for testing
/// @notice Simulates ICS27GMP behavior for unit tests
contract MockICS27GMP {
    uint64 public nextSequence = 1;
    mapping(address account => IICS27GMPMsgs.AccountIdentifier identifier) public accountIdentifiers;

    function sendCall(IICS27GMPMsgs.SendCallMsg calldata) external returns (uint64) {
        uint64 seq = nextSequence;
        ++nextSequence;
        return seq;
    }

    function setNextSequence(uint64 seq) external {
        nextSequence = seq;
    }

    // solhint-disable-next-line gas-calldata-parameters
    function setAccountIdentifier(address account, IICS27GMPMsgs.AccountIdentifier memory id) external {
        accountIdentifiers[account] = id;
    }

    function getAccountIdentifier(address account) external view returns (IICS27GMPMsgs.AccountIdentifier memory) {
        IICS27GMPMsgs.AccountIdentifier memory id = accountIdentifiers[account];
        // solhint-disable-next-line gas-custom-errors
        require(bytes(id.clientId).length > 0, "Account not found");
        return id;
    }
}

contract IFTTest is Test {
    // solhint-disable gas-indexed-events
    TestIFT public ift;
    MockICS27GMP public mockIcs27;
    EVMIFTSendCallConstructor public evmCallConstructor;
    AccessManager public accessManager;

    TestHelper public th = new TestHelper();

    address public authority;
    address public user1;
    address public user2;
    address public relayer;

    string public constant CLIENT_ID = "client-0";
    string public constant COUNTERPARTY_IFT = "0x1234567890123456789012345678901234567890";

    uint256 public constant INITIAL_BALANCE = 1000 ether;

    event IFTBridgeRegistered(string clientId, string counterpartyIFTAddress, address iftSendCallConstructor);
    event IFTTransferInitiated(
        string clientId, uint64 sequence, address indexed sender, string receiver, uint256 amount
    );
    event IFTMintReceived(string clientId, address indexed receiver, uint256 amount);
    event IFTTransferCompleted(string clientId, uint64 sequence, address indexed sender, uint256 amount);
    event IFTTransferRefunded(string clientId, uint64 sequence, address indexed sender, uint256 amount);

    function setUp() public {
        authority = makeAddr("authority");
        user1 = makeAddr("user1");
        user2 = makeAddr("user2");
        relayer = makeAddr("relayer");

        accessManager = new AccessManager(authority);
        mockIcs27 = new MockICS27GMP();
        evmCallConstructor = new EVMIFTSendCallConstructor();

        ift = new TestIFT(IICS27GMP(address(mockIcs27)), address(accessManager));

        // Grant IFT the ability to call registerIFTBridge (role 0 = admin by default)
        vm.startPrank(authority);
        bytes4 registerSelector = IIFT.registerIFTBridge.selector;
        accessManager.setTargetFunctionRole(address(ift), _asSingletonArray(registerSelector), 0);
        vm.stopPrank();

        // Give user1 some tokens
        ift.mint(user1, INITIAL_BALANCE);
    }

    // Helper Functions

    function _asSingletonArray(bytes4 element) internal pure returns (bytes4[] memory) {
        bytes4[] memory array = new bytes4[](1);
        array[0] = element;
        return array;
    }

    function _registerBridge() internal {
        vm.prank(authority);
        ift.registerIFTBridge(CLIENT_ID, COUNTERPARTY_IFT, address(evmCallConstructor));
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
            payload: IICS26RouterMsgs.Payload({ sourcePort: "", destPort: "", version: "", encoding: "", value: "" }),
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
            payload: IICS26RouterMsgs.Payload({ sourcePort: "", destPort: "", version: "", encoding: "", value: "" }),
            relayer: relayer
        });
    }

    // registerIFTBridge Tests

    function test_registerIFTBridge_success() public {
        vm.expectEmit(true, true, true, true);
        emit IFTBridgeRegistered(CLIENT_ID, COUNTERPARTY_IFT, address(evmCallConstructor));

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

        uint256 balanceBefore = ift.balanceOf(user1);

        vm.expectEmit(true, true, true, true);
        emit IFTTransferInitiated(CLIENT_ID, expectedSeq, user1, receiver, transferAmount);

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
        ift.iftTransfer(CLIENT_ID, receiver, amount1, timeout);
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

        uint256 balanceBefore = ift.balanceOf(user1);

        vm.expectEmit(true, true, true, true);
        emit IFTTransferInitiated(CLIENT_ID, expectedSeq, user1, receiver, transferAmount);

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

        vm.prank(user1);
        ift.iftTransfer(CLIENT_ID, receiver, amount, timeout);

        assertEq(ift.balanceOf(user1), INITIAL_BALANCE - amount);
    }

    // iftMint Tests

    function test_iftMint_success() public {
        _registerBridge();

        address ics27Account = makeAddr("ics27Account");
        uint256 mintAmount = 200 ether;

        mockIcs27.setAccountIdentifier(
            ics27Account, IICS27GMPMsgs.AccountIdentifier({ clientId: CLIENT_ID, sender: COUNTERPARTY_IFT, salt: "" })
        );

        uint256 balanceBefore = ift.balanceOf(user2);

        vm.expectEmit(true, true, true, true);
        emit IFTMintReceived(CLIENT_ID, user2, mintAmount);

        vm.prank(ics27Account);
        ift.iftMint(user2, mintAmount);

        assertEq(ift.balanceOf(user2), balanceBefore + mintAmount);
    }

    function test_iftMint_bridgeNotFound_reverts() public {
        address ics27Account = makeAddr("ics27Account");

        mockIcs27.setAccountIdentifier(
            ics27Account,
            IICS27GMPMsgs.AccountIdentifier({ clientId: "unknown-client", sender: COUNTERPARTY_IFT, salt: "" })
        );

        vm.prank(ics27Account);
        vm.expectRevert(abi.encodeWithSelector(IIFTErrors.IFTBridgeNotFound.selector, "unknown-client"));
        ift.iftMint(user2, 100 ether);
    }

    function test_iftMint_unauthorizedSender_reverts() public {
        _registerBridge();

        address ics27Account = makeAddr("ics27Account");
        string memory wrongSender = "0xwrongSenderAddress12345678901234567890";

        mockIcs27.setAccountIdentifier(
            ics27Account, IICS27GMPMsgs.AccountIdentifier({ clientId: CLIENT_ID, sender: wrongSender, salt: "" })
        );

        vm.prank(ics27Account);
        vm.expectRevert(abi.encodeWithSelector(IIFTErrors.IFTUnauthorizedMint.selector, COUNTERPARTY_IFT, wrongSender));
        ift.iftMint(user2, 100 ether);
    }

    function test_iftMint_unexpectedSalt_reverts() public {
        _registerBridge();

        address ics27Account = makeAddr("ics27Account");
        bytes memory unexpectedSalt = hex"1234";

        mockIcs27.setAccountIdentifier(
            ics27Account,
            IICS27GMPMsgs.AccountIdentifier({ clientId: CLIENT_ID, sender: COUNTERPARTY_IFT, salt: unexpectedSalt })
        );

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

        mockIcs27.setAccountIdentifier(
            ics27Account, IICS27GMPMsgs.AccountIdentifier({ clientId: CLIENT_ID, sender: COUNTERPARTY_IFT, salt: "" })
        );

        vm.prank(ics27Account);
        ift.iftMint(user2, amount);

        assertEq(ift.balanceOf(user2), amount);
    }

    function test_iftMint_zeroAmount() public {
        _registerBridge();

        address ics27Account = makeAddr("ics27Account");

        mockIcs27.setAccountIdentifier(
            ics27Account, IICS27GMPMsgs.AccountIdentifier({ clientId: CLIENT_ID, sender: COUNTERPARTY_IFT, salt: "" })
        );

        // Zero amount mint is allowed by ERC20 (no-op)
        vm.prank(ics27Account);
        ift.iftMint(user2, 0);

        assertEq(ift.balanceOf(user2), 0);
    }

    function test_iftMint_toZeroAddress_reverts() public {
        _registerBridge();

        address ics27Account = makeAddr("ics27Account");

        mockIcs27.setAccountIdentifier(
            ics27Account, IICS27GMPMsgs.AccountIdentifier({ clientId: CLIENT_ID, sender: COUNTERPARTY_IFT, salt: "" })
        );

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

        vm.prank(user1);
        ift.iftTransfer(CLIENT_ID, receiver, transferAmount, timeout);

        uint64 seq = 1;
        bytes memory successAck = hex"01";

        IIBCAppCallbacks.OnAcknowledgementPacketCallback memory ackMsg = _createAckCallback(CLIENT_ID, seq, successAck);

        vm.expectEmit(true, true, true, true);
        emit IFTTransferCompleted(CLIENT_ID, seq, user1, transferAmount);

        vm.prank(address(mockIcs27));
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

        vm.prank(user1);
        ift.iftTransfer(CLIENT_ID, receiver, transferAmount, timeout);

        uint256 balanceAfterTransfer = ift.balanceOf(user1);
        assertEq(balanceAfterTransfer, INITIAL_BALANCE - transferAmount);

        uint64 seq = 1;
        bytes memory errorAck = ICS24Host.UNIVERSAL_ERROR_ACK;

        IIBCAppCallbacks.OnAcknowledgementPacketCallback memory ackMsg = _createAckCallback(CLIENT_ID, seq, errorAck);

        vm.expectEmit(true, true, true, true);
        emit IFTTransferRefunded(CLIENT_ID, seq, user1, transferAmount);

        vm.prank(address(mockIcs27));
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

        vm.prank(address(mockIcs27));
        vm.expectRevert(abi.encodeWithSelector(IIFTErrors.IFTPendingTransferNotFound.selector, CLIENT_ID, 999));
        ift.onAckPacket(false, ackMsg);
    }

    function test_onAckPacket_doubleAck_reverts() public {
        _registerBridge();

        uint256 transferAmount = 100 ether;
        string memory receiver = Strings.toHexString(user2);
        uint64 timeout = th.DEFAULT_TIMEOUT_TIMESTAMP();

        vm.prank(user1);
        ift.iftTransfer(CLIENT_ID, receiver, transferAmount, timeout);

        uint64 seq = 1;
        bytes memory successAck = hex"01";

        IIBCAppCallbacks.OnAcknowledgementPacketCallback memory ackMsg = _createAckCallback(CLIENT_ID, seq, successAck);

        // First ack succeeds
        vm.prank(address(mockIcs27));
        ift.onAckPacket(true, ackMsg);

        // Second ack should fail (pending transfer already cleared)
        // With success=true, it just deletes and emits (no revert on empty)
        // With success=false, it tries to refund and reverts
        vm.prank(address(mockIcs27));
        vm.expectRevert(abi.encodeWithSelector(IIFTErrors.IFTPendingTransferNotFound.selector, CLIENT_ID, seq));
        ift.onAckPacket(false, ackMsg);
    }

    function test_onAckPacket_success_noPendingTransfer_reverts() public {
        // When success=true and no pending transfer exists, it should revert
        IIBCAppCallbacks.OnAcknowledgementPacketCallback memory ackMsg = _createAckCallback(CLIENT_ID, 999, hex"01");

        vm.prank(address(mockIcs27));
        vm.expectRevert(abi.encodeWithSelector(IIFTErrors.IFTPendingTransferNotFound.selector, CLIENT_ID, 999));
        ift.onAckPacket(true, ackMsg);
    }

    // onTimeoutPacket Tests

    function test_onTimeoutPacket_refundsTransfer() public {
        _registerBridge();

        uint256 transferAmount = 100 ether;
        string memory receiver = Strings.toHexString(user2);
        uint64 timeout = th.DEFAULT_TIMEOUT_TIMESTAMP();

        vm.prank(user1);
        ift.iftTransfer(CLIENT_ID, receiver, transferAmount, timeout);

        uint256 balanceAfterTransfer = ift.balanceOf(user1);
        assertEq(balanceAfterTransfer, INITIAL_BALANCE - transferAmount);

        uint64 seq = 1;

        IIBCAppCallbacks.OnTimeoutPacketCallback memory timeoutMsg = _createTimeoutCallback(CLIENT_ID, seq);

        vm.expectEmit(true, true, true, true);
        emit IFTTransferRefunded(CLIENT_ID, seq, user1, transferAmount);

        vm.prank(address(mockIcs27));
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

        vm.prank(address(mockIcs27));
        vm.expectRevert(abi.encodeWithSelector(IIFTErrors.IFTPendingTransferNotFound.selector, CLIENT_ID, 999));
        ift.onTimeoutPacket(timeoutMsg);
    }

    function test_onTimeoutPacket_doubleTimeout_reverts() public {
        _registerBridge();

        uint256 transferAmount = 100 ether;
        string memory receiver = Strings.toHexString(user2);
        uint64 timeout = th.DEFAULT_TIMEOUT_TIMESTAMP();

        vm.prank(user1);
        ift.iftTransfer(CLIENT_ID, receiver, transferAmount, timeout);

        uint64 seq = 1;
        IIBCAppCallbacks.OnTimeoutPacketCallback memory timeoutMsg = _createTimeoutCallback(CLIENT_ID, seq);

        // First timeout succeeds
        vm.prank(address(mockIcs27));
        ift.onTimeoutPacket(timeoutMsg);

        // Second timeout should fail (pending transfer already cleared)
        vm.prank(address(mockIcs27));
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
        assertEq(address(ift.ics27()), address(mockIcs27));
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

        // Step 1: User1 initiates transfer
        vm.prank(user1);
        ift.iftTransfer(CLIENT_ID, receiver, transferAmount, timeout);

        assertEq(ift.balanceOf(user1), INITIAL_BALANCE - transferAmount);

        // Step 2: Simulate successful ack
        IIBCAppCallbacks.OnAcknowledgementPacketCallback memory ackMsg = _createAckCallback(CLIENT_ID, 1, hex"01");

        vm.prank(address(mockIcs27));
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

        // Step 1: User1 initiates transfer
        vm.prank(user1);
        ift.iftTransfer(CLIENT_ID, receiver, transferAmount, timeout);

        assertEq(ift.balanceOf(user1), INITIAL_BALANCE - transferAmount);

        // Step 2: Simulate timeout
        IIBCAppCallbacks.OnTimeoutPacketCallback memory timeoutMsg = _createTimeoutCallback(CLIENT_ID, 1);

        vm.prank(address(mockIcs27));
        ift.onTimeoutPacket(timeoutMsg);

        // User should get refund
        assertEq(ift.balanceOf(user1), INITIAL_BALANCE);
    }

    function test_fullMintCycle() public {
        _registerBridge();

        uint256 mintAmount = 500 ether;
        address ics27Account = makeAddr("ics27Account");

        mockIcs27.setAccountIdentifier(
            ics27Account, IICS27GMPMsgs.AccountIdentifier({ clientId: CLIENT_ID, sender: COUNTERPARTY_IFT, salt: "" })
        );

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
        ift.iftTransfer(CLIENT_ID, receiver, 100 ether, timeout);
        ift.iftTransfer(clientId2, receiver, 200 ether, timeout);
        vm.stopPrank();

        // Both pending transfers should exist independently
        IIFTMsgs.PendingTransfer memory pending1 = ift.getPendingTransfer(CLIENT_ID, 1);
        IIFTMsgs.PendingTransfer memory pending2 = ift.getPendingTransfer(clientId2, 2);

        assertEq(pending1.amount, 100 ether);
        assertEq(pending2.amount, 200 ether);

        // Ack one, timeout the other
        vm.startPrank(address(mockIcs27));
        ift.onAckPacket(true, _createAckCallback(CLIENT_ID, 1, hex"01"));
        ift.onTimeoutPacket(_createTimeoutCallback(clientId2, 2));
        vm.stopPrank();

        // First transfer completed, second refunded
        assertEq(ift.balanceOf(user1), INITIAL_BALANCE - 100 ether);
    }
}
