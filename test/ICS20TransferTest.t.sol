// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

// solhint-disable custom-errors,max-line-length

import { Test } from "forge-std/Test.sol";
import { IICS26RouterMsgs } from "../src/msgs/IICS26RouterMsgs.sol";
import { IICS26Router } from "../src/interfaces/IICS26Router.sol";
import { IIBCAppCallbacks } from "../src/msgs/IIBCAppCallbacks.sol";
import { IICS20Transfer } from "../src/interfaces/IICS20Transfer.sol";
import { IICS20TransferMsgs } from "../src/msgs/IICS20TransferMsgs.sol";
import { ICS20Transfer } from "../src/ICS20Transfer.sol";
import { TestERC20, MalfunctioningERC20 } from "./mocks/TestERC20.sol";
import { IBCERC20 } from "../src/utils/IBCERC20.sol";
import { IERC20Errors } from "@openzeppelin/interfaces/draft-IERC6093.sol";
import { ICS20Lib } from "../src/utils/ICS20Lib.sol";
import { IICS20Errors } from "../src/errors/IICS20Errors.sol";
import { Strings } from "@openzeppelin/utils/Strings.sol";
import { Vm } from "forge-std/Vm.sol";
import { Ownable } from "@openzeppelin/access/Ownable.sol";

contract ICS20TransferTest is Test {
    ICS20Transfer public ics20Transfer;
    TestERC20 public erc20;
    string public erc20AddressStr;

    address public sender;
    string public senderStr;
    address public receiver;
    string public receiverStr = "receiver";

    /// @dev the default send amount for sendTransfer
    uint256 public defaultAmount = 1_000_000_100_000_000_001;
    /// @dev the default sdkCoin amount when sending from the sdk side
    uint256 public defaultSdkCoinAmount = 1_000_000;

    ICS20Lib.FungibleTokenPacketData public defaultSendPacketData;
    bytes public data;

    function setUp() public {
        ics20Transfer = new ICS20Transfer(address(this));
        erc20 = new TestERC20();

        sender = makeAddr("sender");

        erc20AddressStr = Strings.toHexString(address(erc20));
        senderStr = Strings.toHexString(sender);

        defaultSendPacketData = ICS20Lib.FungibleTokenPacketData({
            denom: erc20AddressStr,
            sender: senderStr,
            receiver: receiverStr,
            amount: defaultAmount,
            memo: "memo"
        });

        data = ICS20Lib.encodePayload(defaultSendPacketData);
    }

    function test_success_sendTransfer() public {
        IICS26RouterMsgs.Packet memory packet = _getTestPacket();

        IICS20TransferMsgs.SendTransferMsg memory msgSendTransfer = IICS20TransferMsgs.SendTransferMsg({
            denom: erc20AddressStr,
            amount: defaultAmount,
            receiver: receiverStr,
            sourceChannel: packet.sourceChannel,
            destPort: packet.payloads[0].sourcePort,
            timeoutTimestamp: uint64(block.timestamp + 1000),
            memo: "memo"
        });

        vm.mockCall(address(this), abi.encodeWithSelector(IICS26Router.sendPacket.selector), abi.encode(uint32(42)));
        vm.prank(sender);
        uint32 sequence = ics20Transfer.sendTransfer(msgSendTransfer);
        assertEq(sequence, 42);
    }

    function test_failure_sendTransfer() public {
        IICS26RouterMsgs.Packet memory packet = _getTestPacket();

        // just to make sure it doesn't accidentally revert on the router call
        vm.mockCall(address(this), abi.encodeWithSelector(IICS26Router.sendPacket.selector), abi.encode(uint32(42)));

        vm.startPrank(sender);

        IICS20TransferMsgs.SendTransferMsg memory msgSendTransfer = IICS20TransferMsgs.SendTransferMsg({
            denom: erc20AddressStr,
            amount: defaultAmount,
            receiver: receiverStr,
            sourceChannel: packet.sourceChannel,
            destPort: packet.payloads[0].sourcePort,
            timeoutTimestamp: uint64(block.timestamp + 1000),
            memo: "memo"
        });

        // just to prove that it works with the unaltered transfer message
        uint32 sequence = ics20Transfer.sendTransfer(msgSendTransfer);
        assertEq(sequence, 42);

        // initial amount is zero
        msgSendTransfer.amount = 0;
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20InvalidAmount.selector, 0));
        ics20Transfer.sendTransfer(msgSendTransfer);
        // reset amount
        msgSendTransfer.amount = defaultAmount;

        // denom is not an address
        msgSendTransfer.denom = "notanaddress";
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20InvalidAddress.selector, "notanaddress"));
        ics20Transfer.sendTransfer(msgSendTransfer);
        // reset denom
        msgSendTransfer.denom = erc20AddressStr;
    }

    function test_success_onSendPacket_from_sender() public {
        IICS26RouterMsgs.Packet memory packet = _getTestPacket();

        erc20.mint(sender, defaultAmount);
        vm.prank(sender);
        erc20.approve(address(ics20Transfer), defaultAmount);

        uint256 senderBalanceBefore = erc20.balanceOf(sender);
        uint256 contractBalanceBefore = erc20.balanceOf(ics20Transfer.escrow());
        assertEq(senderBalanceBefore, defaultAmount);
        assertEq(contractBalanceBefore, 0);

        vm.expectEmit();
        emit IICS20Transfer.ICS20Transfer(defaultSendPacketData, address(erc20));
        ics20Transfer.onSendPacket(
            IIBCAppCallbacks.OnSendPacketCallback({
                sourceChannel: packet.sourceChannel,
                destinationChannel: packet.destChannel,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                sender: sender
            })
        );

        uint256 senderBalanceAfter = erc20.balanceOf(sender);
        uint256 contractBalanceAfter = erc20.balanceOf(ics20Transfer.escrow());
        assertEq(senderBalanceAfter, 0);
        assertEq(contractBalanceAfter, defaultAmount);
    }

    function test_success_onSendPacket_from_ics20() public {
        IICS26RouterMsgs.Packet memory packet = _getTestPacket();

        erc20.mint(sender, defaultAmount);
        vm.prank(sender);
        erc20.approve(address(ics20Transfer), defaultAmount);

        uint256 senderBalanceBefore = erc20.balanceOf(sender);
        uint256 contractBalanceBefore = erc20.balanceOf(ics20Transfer.escrow());
        assertEq(senderBalanceBefore, defaultAmount);
        assertEq(contractBalanceBefore, 0);

        ICS20Lib.FungibleTokenPacketData memory pd;
        pd.denom = erc20AddressStr;
        pd.sender = senderStr;
        pd.amount = defaultAmount;
        pd.receiver = receiverStr;
        pd.memo = "memo";
        //data = ICS20Lib.encodePayload(erc20AddressStr, defaultAmount, senderStr, receiverStr, "memo");

        data = ICS20Lib.encodePayload(pd);

        vm.expectEmit();
        emit IICS20Transfer.ICS20Transfer(defaultSendPacketData, address(erc20));
        ics20Transfer.onSendPacket(
            IIBCAppCallbacks.OnSendPacketCallback({
                sourceChannel: packet.sourceChannel,
                destinationChannel: packet.destChannel,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                sender: address(ics20Transfer)
            })
        );

        uint256 senderBalanceAfter = erc20.balanceOf(sender);
        uint256 contractBalanceAfter = erc20.balanceOf(ics20Transfer.escrow());
        assertEq(senderBalanceAfter, 0);
        assertEq(contractBalanceAfter, defaultAmount);
    }

    function test_success_onSendPacketWithLargeAmount() public {
        IICS26RouterMsgs.Packet memory packet = _getTestPacket();

        uint256 largeAmount = 1_000_000_000_000_000_001_000_000_000_000;

        erc20.mint(sender, largeAmount);
        vm.prank(sender);
        erc20.approve(address(ics20Transfer), largeAmount);

        uint256 senderBalanceBefore = erc20.balanceOf(sender);
        uint256 contractBalanceBefore = erc20.balanceOf(ics20Transfer.escrow());
        assertEq(senderBalanceBefore, largeAmount);
        assertEq(contractBalanceBefore, 0);

        defaultSendPacketData.amount = largeAmount;

        data = ICS20Lib.encodePayload(defaultSendPacketData);
        packet.payloads[0].value = data;

        vm.expectEmit();
        emit IICS20Transfer.ICS20Transfer(defaultSendPacketData, address(erc20));
        ics20Transfer.onSendPacket(
            IIBCAppCallbacks.OnSendPacketCallback({
                sourceChannel: packet.sourceChannel,
                destinationChannel: packet.destChannel,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                sender: address(ics20Transfer)
            })
        );

        assertEq(erc20.balanceOf(sender), 0);
        assertEq(erc20.balanceOf(ics20Transfer.escrow()), largeAmount);
    }

    function test_failure_onSendPacket() public {
        IICS26RouterMsgs.Packet memory packet = _getTestPacket();

        // this contract acts as the ics26Router (it is the address given as owner to the ics20Transfer contract)

        // test missing approval
        vm.expectRevert(
            abi.encodeWithSelector(
                IERC20Errors.ERC20InsufficientAllowance.selector, address(ics20Transfer), 0, defaultAmount
            )
        );
        ics20Transfer.onSendPacket(
            IIBCAppCallbacks.OnSendPacketCallback({
                sourceChannel: packet.sourceChannel,
                destinationChannel: packet.destChannel,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                sender: sender
            })
        );

        // test insufficient balance
        vm.prank(sender);
        erc20.approve(address(ics20Transfer), defaultAmount);
        vm.expectRevert(
            abi.encodeWithSelector(IERC20Errors.ERC20InsufficientBalance.selector, sender, 0, defaultAmount)
        );
        ics20Transfer.onSendPacket(
            IIBCAppCallbacks.OnSendPacketCallback({
                sourceChannel: packet.sourceChannel,
                destinationChannel: packet.destChannel,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                sender: sender
            })
        );

        // test invalid amount
        defaultSendPacketData.amount = 0;
        packet.payloads[0].value = ICS20Lib.encodePayload(defaultSendPacketData);
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20InvalidAmount.selector, 0));
        ics20Transfer.onSendPacket(
            IIBCAppCallbacks.OnSendPacketCallback({
                sourceChannel: packet.sourceChannel,
                destinationChannel: packet.destChannel,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                sender: sender
            })
        );
        defaultSendPacketData.amount = defaultAmount; // reset amount
        packet.payloads[0].value = ICS20Lib.encodePayload(defaultSendPacketData);

        // test invalid data
        packet.payloads[0].value = bytes("invalid");
        vm.expectRevert(); // Given the data is invalid, we expect the abi.decodePayload to fail with a generic revert
        ics20Transfer.onSendPacket(
            IIBCAppCallbacks.OnSendPacketCallback({
                sourceChannel: packet.sourceChannel,
                destinationChannel: packet.destChannel,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                sender: sender
            })
        );

        // test invalid sender
        defaultSendPacketData.sender = "invalid";
        packet.payloads[0].value = ICS20Lib.encodePayload(defaultSendPacketData);
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20InvalidAddress.selector, "invalid"));
        ics20Transfer.onSendPacket(
            IIBCAppCallbacks.OnSendPacketCallback({
                sourceChannel: packet.sourceChannel,
                destinationChannel: packet.destChannel,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                sender: sender
            })
        );
        defaultSendPacketData.sender = senderStr; // reset sender
        packet.payloads[0].value = ICS20Lib.encodePayload(defaultSendPacketData);

        // test packet sender is not the same as the payload sender
        address notSender = makeAddr("notSender");
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20UnauthorizedPacketSender.selector, notSender));
        ics20Transfer.onSendPacket(
            IIBCAppCallbacks.OnSendPacketCallback({
                sourceChannel: packet.sourceChannel,
                destinationChannel: packet.destChannel,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                sender: notSender // not the same as the payload sender
             })
        );

        // test msg sender is sender, i.e. not owner (ics26Router)
        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, sender));
        vm.prank(sender);
        ics20Transfer.onSendPacket(
            IIBCAppCallbacks.OnSendPacketCallback({
                sourceChannel: packet.sourceChannel,
                destinationChannel: packet.destChannel,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                sender: sender
            })
        );

        // test msg sender is someone else entierly, i.e. owner (ics26Router)
        address someoneElse = makeAddr("someoneElse");
        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, someoneElse));
        vm.prank(someoneElse);
        ics20Transfer.onSendPacket(
            IIBCAppCallbacks.OnSendPacketCallback({
                sourceChannel: packet.sourceChannel,
                destinationChannel: packet.destChannel,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                sender: sender
            })
        );

        // test invalid token contract
        defaultSendPacketData.denom = "invalid";
        packet.payloads[0].value = ICS20Lib.encodePayload(defaultSendPacketData);
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20InvalidAddress.selector, "invalid"));
        ics20Transfer.onSendPacket(
            IIBCAppCallbacks.OnSendPacketCallback({
                sourceChannel: packet.sourceChannel,
                destinationChannel: packet.destChannel,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                sender: sender
            })
        );
        defaultSendPacketData.denom = erc20AddressStr; // reset denom
        packet.payloads[0].value = ICS20Lib.encodePayload(defaultSendPacketData);

        // test invalid version
        packet.payloads[0].version = "invalid";
        packet.payloads[0].value = ICS20Lib.encodePayload(defaultSendPacketData);
        vm.expectRevert(
            abi.encodeWithSelector(IICS20Errors.ICS20UnexpectedVersion.selector, ICS20Lib.ICS20_VERSION, "invalid")
        );
        ics20Transfer.onSendPacket(
            IIBCAppCallbacks.OnSendPacketCallback({
                sourceChannel: packet.sourceChannel,
                destinationChannel: packet.destChannel,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                sender: sender
            })
        );
        packet.payloads[0].version = ICS20Lib.ICS20_VERSION; // reset version

        // test malfunctioning transfer
        MalfunctioningERC20 malfunctioningERC20 = new MalfunctioningERC20();
        malfunctioningERC20.mint(sender, defaultAmount);
        malfunctioningERC20.setMalfunction(true); // Turn on the malfunctioning behaviour (no update)
        vm.prank(sender);
        malfunctioningERC20.approve(address(ics20Transfer), defaultAmount);
        string memory malfuncERC20AddressStr = Strings.toHexString(address(malfunctioningERC20));

        defaultSendPacketData.denom = malfuncERC20AddressStr;
        packet.payloads[0].value = ICS20Lib.encodePayload(defaultSendPacketData);
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20UnexpectedERC20Balance.selector, defaultAmount, 0));
        ics20Transfer.onSendPacket(
            IIBCAppCallbacks.OnSendPacketCallback({
                sourceChannel: packet.sourceChannel,
                destinationChannel: packet.destChannel,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                sender: address(ics20Transfer)
            })
        );
    }

    function test_success_onAcknowledgementPacketWithSuccessAck() public {
        IICS26RouterMsgs.Packet memory packet = _getTestPacket();

        erc20.mint(sender, defaultAmount);
        vm.prank(sender);
        erc20.approve(address(ics20Transfer), defaultAmount);

        uint256 senderBalanceBefore = erc20.balanceOf(sender);
        uint256 contractBalanceBefore = erc20.balanceOf(ics20Transfer.escrow());
        assertEq(senderBalanceBefore, defaultAmount);
        assertEq(contractBalanceBefore, 0);

        vm.expectEmit();
        emit IICS20Transfer.ICS20Transfer(defaultSendPacketData, address(erc20));
        ics20Transfer.onSendPacket(
            IIBCAppCallbacks.OnSendPacketCallback({
                sourceChannel: packet.sourceChannel,
                destinationChannel: packet.destChannel,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                sender: address(ics20Transfer)
            })
        );

        uint256 senderBalanceAfterSend = erc20.balanceOf(sender);
        uint256 contractBalanceAfterSend = erc20.balanceOf(ics20Transfer.escrow());
        assertEq(senderBalanceAfterSend, 0);
        assertEq(contractBalanceAfterSend, defaultAmount);

        vm.expectEmit();
        emit IICS20Transfer.ICS20Acknowledgement(defaultSendPacketData, ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON);
        ics20Transfer.onAcknowledgementPacket(
            IIBCAppCallbacks.OnAcknowledgementPacketCallback({
                sourceChannel: packet.sourceChannel,
                destinationChannel: packet.destChannel,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                acknowledgement: ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON,
                relayer: makeAddr("relayer")
            })
        );

        // Nothing should change
        uint256 senderBalanceAfterAck = erc20.balanceOf(sender);
        uint256 contractBalanceAfterAck = erc20.balanceOf(ics20Transfer.escrow());
        assertEq(senderBalanceAfterAck, 0);
        assertEq(contractBalanceAfterAck, defaultAmount);
    }

    function test_success_onAcknowledgementPacketWithFailedAck() public {
        IICS26RouterMsgs.Packet memory packet = _getTestPacket();

        erc20.mint(sender, defaultAmount);
        vm.prank(sender);
        erc20.approve(address(ics20Transfer), defaultAmount);

        uint256 senderBalanceBefore = erc20.balanceOf(sender);
        uint256 contractBalanceBefore = erc20.balanceOf(ics20Transfer.escrow());
        assertEq(senderBalanceBefore, defaultAmount);
        assertEq(contractBalanceBefore, 0);

        vm.expectEmit();
        emit IICS20Transfer.ICS20Transfer(defaultSendPacketData, address(erc20));
        ics20Transfer.onSendPacket(
            IIBCAppCallbacks.OnSendPacketCallback({
                sourceChannel: packet.sourceChannel,
                destinationChannel: packet.destChannel,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                sender: address(ics20Transfer)
            })
        );

        uint256 senderBalanceAfterSend = erc20.balanceOf(sender);
        uint256 contractBalanceAfterSend = erc20.balanceOf(ics20Transfer.escrow());
        assertEq(senderBalanceAfterSend, 0);
        assertEq(contractBalanceAfterSend, defaultAmount);

        vm.expectEmit();
        emit IICS20Transfer.ICS20Acknowledgement(defaultSendPacketData, ICS20Lib.FAILED_ACKNOWLEDGEMENT_JSON);
        ics20Transfer.onAcknowledgementPacket(
            IIBCAppCallbacks.OnAcknowledgementPacketCallback({
                sourceChannel: packet.sourceChannel,
                destinationChannel: packet.destChannel,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                acknowledgement: ICS20Lib.FAILED_ACKNOWLEDGEMENT_JSON,
                relayer: makeAddr("relayer")
            })
        );

        // transfer should be reverted
        uint256 senderBalanceAfterAck = erc20.balanceOf(sender);
        uint256 contractBalanceAfterAck = erc20.balanceOf(ics20Transfer.escrow());
        assertEq(senderBalanceAfterAck, defaultAmount);
        assertEq(contractBalanceAfterAck, 0);
    }

    function test_failure_onAcknowledgementPacket() public {
        IICS26RouterMsgs.Packet memory packet = _getTestPacket();

        // test invalid data
        data = bytes("invalid");
        packet.payloads[0].value = data;
        vm.expectRevert(bytes(""));
        ics20Transfer.onAcknowledgementPacket(
            IIBCAppCallbacks.OnAcknowledgementPacketCallback({
                sourceChannel: packet.sourceChannel,
                destinationChannel: packet.destChannel,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                acknowledgement: ICS20Lib.FAILED_ACKNOWLEDGEMENT_JSON,
                relayer: makeAddr("relayer")
            })
        );

        // test invalid contract

        ICS20Lib.FungibleTokenPacketData memory pd;
        pd.denom = "invalid";
        pd.amount = defaultAmount;
        pd.sender = senderStr;
        pd.receiver = receiverStr;
        pd.memo = "memo";

        //data = ICS20Lib.marshalJSON("invalid", defaultAmount, senderStr, receiverStr, "memo");
        data = ICS20Lib.encodePayload(pd);

        packet.payloads[0].value = data;
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20InvalidAddress.selector, "invalid"));
        ics20Transfer.onAcknowledgementPacket(
            IIBCAppCallbacks.OnAcknowledgementPacketCallback({
                sourceChannel: packet.sourceChannel,
                destinationChannel: packet.destChannel,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                acknowledgement: ICS20Lib.FAILED_ACKNOWLEDGEMENT_JSON,
                relayer: makeAddr("relayer")
            })
        );

        pd.denom = erc20AddressStr;
        pd.amount = defaultAmount;
        pd.sender = "invalid";
        pd.receiver = receiverStr;
        pd.memo = "memo";

        data = ICS20Lib.encodePayload(pd);

        // test invalid sender
        //data = ICS20Lib.marshalJSON(erc20AddressStr, defaultAmount, "invalid", receiverStr, "memo");
        packet.payloads[0].value = data;
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20InvalidAddress.selector, "invalid"));
        ics20Transfer.onAcknowledgementPacket(
            IIBCAppCallbacks.OnAcknowledgementPacketCallback({
                sourceChannel: packet.sourceChannel,
                destinationChannel: packet.destChannel,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                acknowledgement: ICS20Lib.FAILED_ACKNOWLEDGEMENT_JSON,
                relayer: makeAddr("relayer")
            })
        );
    }

    function test_success_onTimeoutPacket() public {
        IICS26RouterMsgs.Packet memory packet = _getTestPacket();

        erc20.mint(sender, defaultAmount);
        vm.prank(sender);
        erc20.approve(address(ics20Transfer), defaultAmount);

        uint256 senderBalanceBefore = erc20.balanceOf(sender);
        uint256 contractBalanceBefore = erc20.balanceOf(ics20Transfer.escrow());
        assertEq(senderBalanceBefore, defaultAmount);
        assertEq(contractBalanceBefore, 0);

        vm.expectEmit();
        emit IICS20Transfer.ICS20Transfer(defaultSendPacketData, address(erc20));
        ics20Transfer.onSendPacket(
            IIBCAppCallbacks.OnSendPacketCallback({
                sourceChannel: packet.sourceChannel,
                destinationChannel: packet.destChannel,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                sender: address(ics20Transfer)
            })
        );

        uint256 senderBalanceAfterSend = erc20.balanceOf(sender);
        uint256 contractBalanceAfterSend = erc20.balanceOf(ics20Transfer.escrow());
        assertEq(senderBalanceAfterSend, 0);
        assertEq(contractBalanceAfterSend, defaultAmount);

        vm.expectEmit();
        emit IICS20Transfer.ICS20Timeout(defaultSendPacketData);
        ics20Transfer.onTimeoutPacket(
            IIBCAppCallbacks.OnTimeoutPacketCallback({
                sourceChannel: packet.sourceChannel,
                destinationChannel: packet.destChannel,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                relayer: makeAddr("relayer")
            })
        );

        // transfer should be reverted
        uint256 senderBalanceAfterAck = erc20.balanceOf(sender);
        uint256 contractBalanceAfterAck = erc20.balanceOf(ics20Transfer.escrow());
        assertEq(senderBalanceAfterAck, defaultAmount);
        assertEq(contractBalanceAfterAck, 0);
    }

    function test_failure_onTimeoutPacket() public {
        IICS26RouterMsgs.Packet memory packet = _getTestPacket();

        // test invalid data
        data = bytes("invalid");
        packet.payloads[0].value = data;
        vm.expectRevert(bytes(""));
        ics20Transfer.onTimeoutPacket(
            IIBCAppCallbacks.OnTimeoutPacketCallback({
                sourceChannel: packet.sourceChannel,
                destinationChannel: packet.destChannel,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                relayer: makeAddr("relayer")
            })
        );

        // test invalid contract
        ICS20Lib.FungibleTokenPacketData memory pd;
        pd.denom = "invalid";
        pd.amount = defaultAmount;
        pd.sender = senderStr;
        pd.receiver = receiverStr;
        pd.memo = "memo";
        //data = ICS20Lib.marshalJSON("invalid", defaultAmount, senderStr, receiverStr, "memo");
        data = ICS20Lib.encodePayload(pd);

        packet.payloads[0].value = data;
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20InvalidAddress.selector, "invalid"));
        ics20Transfer.onTimeoutPacket(
            IIBCAppCallbacks.OnTimeoutPacketCallback({
                sourceChannel: packet.sourceChannel,
                destinationChannel: packet.destChannel,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                relayer: makeAddr("relayer")
            })
        );

        // test invalid sender
        pd.denom = erc20AddressStr;
        pd.amount = defaultAmount;
        pd.sender = "invalid";
        pd.receiver = receiverStr;
        pd.memo = "memo";
        //data = ICS20Lib.marshalJSON("invalid", defaultAmount, senderStr, receiverStr, "memo");
        data = ICS20Lib.encodePayload(pd);

        //data = ICS20Lib.marshalJSON(erc20AddressStr, defaultAmount, "invalid", receiverStr, "memo");
        packet.payloads[0].value = data;
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20InvalidAddress.selector, "invalid"));
        ics20Transfer.onTimeoutPacket(
            IIBCAppCallbacks.OnTimeoutPacketCallback({
                sourceChannel: packet.sourceChannel,
                destinationChannel: packet.destChannel,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                relayer: makeAddr("relayer")
            })
        );
    }

    function test_success_onRecvPacketWithSourceDenom() public {
        IICS26RouterMsgs.Packet memory packet = _getTestPacket();

        erc20.mint(sender, defaultAmount);
        vm.prank(sender);
        erc20.approve(address(ics20Transfer), defaultAmount);

        uint256 senderBalanceBefore = erc20.balanceOf(sender);
        uint256 contractBalanceBefore = erc20.balanceOf(ics20Transfer.escrow());
        assertEq(senderBalanceBefore, defaultAmount);
        assertEq(contractBalanceBefore, 0);

        vm.expectEmit();
        emit IICS20Transfer.ICS20Transfer(defaultSendPacketData, address(erc20));
        ics20Transfer.onSendPacket(
            IIBCAppCallbacks.OnSendPacketCallback({
                sourceChannel: packet.sourceChannel,
                destinationChannel: packet.destChannel,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                sender: address(ics20Transfer)
            })
        );

        uint256 senderBalanceAfterSend = erc20.balanceOf(sender);
        uint256 contractBalanceAfterSend = erc20.balanceOf(ics20Transfer.escrow());
        assertEq(senderBalanceAfterSend, 0);
        assertEq(contractBalanceAfterSend, defaultAmount);

        // Send back (onRecv)
        string memory newSourcePort = packet.payloads[0].destPort;
        string memory newSourceChannel = packet.destChannel;
        string memory receivedDenom =
            string(abi.encodePacked(newSourcePort, "/", newSourceChannel, "/", erc20AddressStr));

        {
            string memory tmpSenderStr = senderStr;
            senderStr = receiverStr;
            receiverStr = tmpSenderStr;
        }

        ICS20Lib.FungibleTokenPacketData memory pd;
        pd.denom = receivedDenom;
        pd.amount = defaultSdkCoinAmount;
        pd.sender = senderStr;
        pd.receiver = receiverStr;
        pd.memo = "memo";

        packet.payloads[0].value = ICS20Lib.encodePayload(pd);
        //ICS20Lib.marshalJSON(receivedDenom, defaultSdkCoinAmount, senderStr, receiverStr, "memo");
        packet.payloads[0].destPort = packet.payloads[0].sourcePort;
        packet.destChannel = packet.sourceChannel;
        packet.payloads[0].sourcePort = newSourcePort;
        packet.sourceChannel = newSourceChannel;

        vm.expectEmit();
        emit IICS20Transfer.ICS20ReceiveTransfer(
            ICS20Lib.FungibleTokenPacketData({
                denom: receivedDenom,
                sender: senderStr,
                receiver: receiverStr,
                amount: defaultSdkCoinAmount,
                memo: "memo"
            }),
            address(erc20)
        );
        bytes memory ack = ics20Transfer.onRecvPacket(
            IIBCAppCallbacks.OnRecvPacketCallback({
                sourceChannel: packet.sourceChannel,
                destinationChannel: packet.destChannel,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                relayer: makeAddr("relayer")
            })
        );
        assertEq(ack, ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON);

        // the tokens should have been transferred back again
        uint256 senderBalanceAfterReceive = erc20.balanceOf(sender);
        uint256 contractBalanceAfterReceive = erc20.balanceOf(ics20Transfer.escrow());
        assertEq(senderBalanceAfterReceive, defaultSdkCoinAmount);
        assertEq(contractBalanceAfterReceive, defaultAmount - defaultSdkCoinAmount);
    }

    function test_success_onRecvPacketWithForeignBaseDenom() public {
        IICS26RouterMsgs.Packet memory packet = _getTestPacket();

        string memory foreignDenom = "uatom";

        senderStr = "cosmos1mhmwgrfrcrdex5gnr0vcqt90wknunsxej63feh";
        receiver = makeAddr("receiver_of_foreign_denom");
        receiverStr = Strings.toHexString(receiver);

        ICS20Lib.FungibleTokenPacketData memory pd;
        pd.denom = foreignDenom;
        pd.amount = defaultAmount;
        pd.sender = senderStr;
        pd.receiver = receiverStr;
        pd.memo = "memo";

        //bytes memory receiveData = ICS20Lib.marshalJSON(foreignDenom, defaultAmount, senderStr, receiverStr, "memo");
        bytes memory receiveData = ICS20Lib.encodePayload(pd);

        packet.payloads[0].value = receiveData;
        packet.payloads[0].destPort = ICS20Lib.DEFAULT_PORT_ID;
        packet.destChannel = "dest-channel";
        packet.payloads[0].sourcePort = ICS20Lib.DEFAULT_PORT_ID;
        packet.sourceChannel = "source-channel";

        string memory expectedFullDenomPath =
            string(abi.encodePacked(packet.payloads[0].destPort, "/", packet.destChannel, "/", foreignDenom));

        vm.expectEmit(true, true, true, false); // Not checking data because we don't know the address yet
        ICS20Lib.FungibleTokenPacketData memory packetData;
        address erc20Address;
        emit IICS20Transfer.ICS20ReceiveTransfer(packetData, erc20Address); // we check these values later
        vm.recordLogs();
        bytes memory ack = ics20Transfer.onRecvPacket(
            IIBCAppCallbacks.OnRecvPacketCallback({
                sourceChannel: packet.sourceChannel,
                destinationChannel: packet.destChannel,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                relayer: makeAddr("relayer")
            })
        );
        assertEq(ack, ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON);

        // find and extract data from the ICS20ReceiveTransfer event
        Vm.Log[] memory entries = vm.getRecordedLogs();
        assertEq(entries.length, 4);
        Vm.Log memory receiveTransferLog = entries[3];
        assertEq(receiveTransferLog.topics[0], IICS20Transfer.ICS20ReceiveTransfer.selector);

        (packetData, erc20Address) = abi.decode(receiveTransferLog.data, (ICS20Lib.FungibleTokenPacketData, address));
        assertEq(packetData.denom, foreignDenom);
        assertNotEq(erc20Address, address(0));
        assertEq(packetData.sender, senderStr);
        assertEq(packetData.receiver, receiverStr);
        assertEq(packetData.amount, defaultAmount);
        assertEq(packetData.memo, "memo");

        IBCERC20 ibcERC20 = IBCERC20(erc20Address);

        // finally, verify the created contract and balances have been updated as expected
        assertEq(ibcERC20.fullDenomPath(), expectedFullDenomPath);
        assertEq(ibcERC20.name(), ICS20Lib.toIBCDenom(expectedFullDenomPath));
        assertEq(ibcERC20.symbol(), foreignDenom);
        assertEq(ibcERC20.totalSupply(), defaultAmount);
        assertEq(ibcERC20.balanceOf(receiver), defaultAmount);
    }

    function test_success_onRecvPacketWithForeignIBCDenom() public {
        IICS26RouterMsgs.Packet memory packet = _getTestPacket();

        string memory foreignDenom = "transfer/channel-42/uatom";

        senderStr = "cosmos1mhmwgrfrcrdex5gnr0vcqt90wknunsxej63feh";
        receiver = makeAddr("receiver_of_foreign_denom");
        receiverStr = Strings.toHexString(receiver);

        ICS20Lib.FungibleTokenPacketData memory pd;
        pd.denom = foreignDenom;
        pd.amount = defaultAmount;
        pd.sender = senderStr;
        pd.receiver = receiverStr;
        pd.memo = "memo";

        //packet.payloads[0].value = ICS20Lib.marshalJSON(foreignDenom, defaultAmount, senderStr, receiverStr, "memo");
        packet.payloads[0].value = ICS20Lib.encodePayload(pd);
        packet.payloads[0].destPort = ICS20Lib.DEFAULT_PORT_ID;
        packet.destChannel = "dest-channel";
        packet.payloads[0].sourcePort = ICS20Lib.DEFAULT_PORT_ID;
        packet.sourceChannel = "source-channel";

        string memory expectedFullDenomPath =
            string(abi.encodePacked(packet.payloads[0].destPort, "/", packet.destChannel, "/", foreignDenom));

        vm.expectEmit(true, true, true, false); // Not checking data because we don't know the address yet
        ICS20Lib.FungibleTokenPacketData memory packetData;
        address erc20Address;
        emit IICS20Transfer.ICS20ReceiveTransfer(packetData, erc20Address); // we check these values later
        vm.recordLogs();
        bytes memory ack = ics20Transfer.onRecvPacket(
            IIBCAppCallbacks.OnRecvPacketCallback({
                sourceChannel: packet.sourceChannel,
                destinationChannel: packet.destChannel,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                relayer: makeAddr("relayer")
            })
        );
        assertEq(ack, ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON);

        // find and extract data from the ICS20ReceiveTransfer event
        Vm.Log[] memory entries = vm.getRecordedLogs();
        assertEq(entries.length, 4);
        Vm.Log memory receiveTransferLog = entries[3];
        assertEq(receiveTransferLog.topics[0], IICS20Transfer.ICS20ReceiveTransfer.selector);

        (packetData, erc20Address) = abi.decode(receiveTransferLog.data, (ICS20Lib.FungibleTokenPacketData, address));
        assertEq(packetData.denom, foreignDenom);
        assertEq(packetData.sender, senderStr);
        assertEq(packetData.receiver, receiverStr);
        assertEq(packetData.amount, defaultAmount);
        assertEq(packetData.memo, "memo");

        IBCERC20 ibcERC20 = IBCERC20(erc20Address);

        // finally, verify balances have been updated as expected
        assertEq(ibcERC20.fullDenomPath(), expectedFullDenomPath);
        assertEq(ibcERC20.name(), ICS20Lib.toIBCDenom(expectedFullDenomPath));
        assertEq(ibcERC20.symbol(), foreignDenom);
        assertEq(ibcERC20.totalSupply(), defaultAmount);
        assertEq(ibcERC20.balanceOf(receiver), defaultAmount);
    }

    function test_failure_onRecvPacket() public {
        IICS26RouterMsgs.Packet memory packet = _getTestPacket();

        string memory ibcDenom =
            string(abi.encodePacked(packet.payloads[0].sourcePort, "/", packet.sourceChannel, "/", erc20AddressStr));

        ICS20Lib.FungibleTokenPacketData memory pd;
        pd.denom = ibcDenom;
        pd.sender = senderStr;
        pd.amount = defaultAmount;
        pd.receiver = receiverStr;
        pd.memo = "memo";

        packet.payloads[0].value = ICS20Lib.encodePayload(pd);

        // test invalid version
        packet.payloads[0].version = "invalid";
        bytes memory ack = ics20Transfer.onRecvPacket(
            IIBCAppCallbacks.OnRecvPacketCallback({
                sourceChannel: packet.sourceChannel,
                destinationChannel: packet.destChannel,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                relayer: makeAddr("relayer")
            })
        );
        assertEq(string(ack), "{\"error\":\"unexpected version: invalid\"}");
        // Reset version
        packet.payloads[0].version = ICS20Lib.ICS20_VERSION;

        // test invalid data
        data = bytes("invalid");
        packet.payloads[0].value = data;
        vm.expectRevert(); // here we expect a generic revert caused by the abi.decodePayload function

        ics20Transfer.onRecvPacket(
            IIBCAppCallbacks.OnRecvPacketCallback({
                sourceChannel: packet.sourceChannel,
                destinationChannel: packet.destChannel,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                relayer: makeAddr("relayer")
            })
        );
        // test invalid amount
        pd.denom = ibcDenom;
        pd.sender = senderStr;
        pd.amount = 0;
        pd.receiver = receiverStr;
        pd.memo = "memo";

        data = ICS20Lib.encodePayload(pd);

        packet.payloads[0].value = data;
        ack = ics20Transfer.onRecvPacket(
            IIBCAppCallbacks.OnRecvPacketCallback({
                sourceChannel: packet.sourceChannel,
                destinationChannel: packet.destChannel,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                relayer: makeAddr("relayer")
            })
        );
        assertEq(string(ack), "{\"error\":\"invalid amount: 0\"}");

        // test receiver chain is source, but denom is not erc20 address
        string memory invalidErc20Denom =
            string(abi.encodePacked(packet.payloads[0].sourcePort, "/", packet.sourceChannel, "/invalid"));

        pd.denom = invalidErc20Denom;
        pd.sender = senderStr;
        pd.amount = defaultAmount;
        pd.receiver = receiverStr;
        pd.memo = "memo";

        data = ICS20Lib.encodePayload(pd);

        packet.payloads[0].value = data;
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20InvalidAddress.selector, "invalid"));
        ics20Transfer.onRecvPacket(
            IIBCAppCallbacks.OnRecvPacketCallback({
                sourceChannel: packet.sourceChannel,
                destinationChannel: packet.destChannel,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                relayer: makeAddr("relayer")
            })
        );

        // test invalid receiver
        pd.denom = ibcDenom;
        pd.sender = senderStr;
        pd.amount = defaultAmount;
        pd.receiver = "invalid";
        pd.memo = "memo";

        data = ICS20Lib.encodePayload(pd);
        packet.payloads[0].value = data;
        ack = ics20Transfer.onRecvPacket(
            IIBCAppCallbacks.OnRecvPacketCallback({
                sourceChannel: packet.sourceChannel,
                destinationChannel: packet.destChannel,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                relayer: makeAddr("relayer")
            })
        );
        assertEq(string(ack), "{\"error\":\"invalid receiver: invalid\"}");

        // just to document current limitations: JSON needs to be in a very specific order
        bytes memory wrongOrderJSON = abi.encodePacked(
            "{\"amount\":\"",
            Strings.toString(defaultAmount),
            "\",\"denom\":\"",
            ibcDenom,
            "\",\"memo\":\"",
            "memo",
            "\",\"receiver\":\"",
            receiverStr,
            "\",\"sender\":\"",
            senderStr,
            "\"}"
        );
        packet.payloads[0].value = wrongOrderJSON;
        vm.expectRevert();
        // abi.encodeWithSelector(
        //      IICS20Errors.ICS20AbiEncodingFailure.selector)//, bytes32("{\"denom\":\""), bytes32("{\"amount\":")
        //)//;
        //);

        ics20Transfer.onRecvPacket(
            IIBCAppCallbacks.OnRecvPacketCallback({
                sourceChannel: packet.sourceChannel,
                destinationChannel: packet.destChannel,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                relayer: makeAddr("relayer")
            })
        );

        //assertEq(string(ack), "{\"error\":\"failed to decode payload\"}");
    }

    function _getTestPacket() internal view returns (IICS26RouterMsgs.Packet memory) {
        IICS26RouterMsgs.Payload[] memory payloads = new IICS26RouterMsgs.Payload[](1);
        payloads[0] = IICS26RouterMsgs.Payload({
            sourcePort: "sourcePort",
            destPort: "destinationPort",
            version: ICS20Lib.ICS20_VERSION,
            encoding: ICS20Lib.ICS20_ENCODING,
            value: data
        });
        return IICS26RouterMsgs.Packet({
            sequence: 0,
            sourceChannel: "sourceChannel",
            destChannel: "destinationChannel",
            timeoutTimestamp: 0,
            payloads: payloads
        });
    }
}
