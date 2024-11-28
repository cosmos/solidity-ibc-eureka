// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

// solhint-disable custom-errors,max-line-length

import { Test } from "forge-std/Test.sol";
import { IICS26RouterMsgs } from "../../contracts/msgs/IICS26RouterMsgs.sol";
import { IICS26Router } from "../../contracts/interfaces/IICS26Router.sol";
import { IIBCAppCallbacks } from "../../contracts/msgs/IIBCAppCallbacks.sol";
import { IICS20Transfer } from "../../contracts/interfaces/IICS20Transfer.sol";
import { IICS20TransferMsgs } from "../../contracts/msgs/IICS20TransferMsgs.sol";
import { ICS20Transfer } from "../../contracts/ICS20Transfer.sol";
import { TestERC20, MalfunctioningERC20 } from "./mocks/TestERC20.sol";
import { IBCERC20 } from "../../contracts/utils/IBCERC20.sol";
import { IERC20Errors } from "@openzeppelin/interfaces/draft-IERC6093.sol";
import { ICS20Lib } from "../../contracts/utils/ICS20Lib.sol";
import { IICS20Errors } from "../../contracts/errors/IICS20Errors.sol";
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

    ICS20Lib.FungibleTokenPacketData public defaultPacketData;
    bytes public data;

    function setUp() public {
        ics20Transfer = new ICS20Transfer(address(this));
        erc20 = new TestERC20();

        sender = makeAddr("sender");

        erc20AddressStr = Strings.toHexString(address(erc20));
        senderStr = Strings.toHexString(sender);

        defaultPacketData = ICS20Lib.FungibleTokenPacketData({
            denom: erc20AddressStr,
            sender: senderStr,
            receiver: receiverStr,
            amount: defaultAmount,
            memo: "memo"
        });

        data = abi.encode(defaultPacketData);
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

        defaultPacketData.amount = largeAmount;

        data = abi.encode(defaultPacketData);
        packet.payloads[0].value = data;

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
        defaultPacketData.amount = 0;
        packet.payloads[0].value = abi.encode(defaultPacketData);
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
        // reset amount
        defaultPacketData.amount = defaultAmount;
        packet.payloads[0].value = abi.encode(defaultPacketData);

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
        // reset data
        packet.payloads[0].value = abi.encode(defaultPacketData);

        // test invalid sender
        defaultPacketData.sender = "invalid";
        packet.payloads[0].value = abi.encode(defaultPacketData);
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
        // reset sender
        defaultPacketData.sender = senderStr;
        packet.payloads[0].value = abi.encode(defaultPacketData);

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
        defaultPacketData.denom = "invalid";
        packet.payloads[0].value = abi.encode(defaultPacketData);
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
        // reset denom
        defaultPacketData.denom = erc20AddressStr;
        packet.payloads[0].value = abi.encode(defaultPacketData);

        // test invalid version
        packet.payloads[0].version = "invalid";
        packet.payloads[0].value = abi.encode(defaultPacketData);
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
        // reset version
        packet.payloads[0].version = ICS20Lib.ICS20_VERSION;

        // test malfunctioning transfer
        MalfunctioningERC20 malfunctioningERC20 = new MalfunctioningERC20();
        malfunctioningERC20.mint(sender, defaultAmount);
        malfunctioningERC20.setMalfunction(true); // Turn on the malfunctioning behaviour (no update)
        vm.prank(sender);
        malfunctioningERC20.approve(address(ics20Transfer), defaultAmount);
        string memory malfuncERC20AddressStr = Strings.toHexString(address(malfunctioningERC20));

        defaultPacketData.denom = malfuncERC20AddressStr;
        packet.payloads[0].value = abi.encode(defaultPacketData);
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
        // reset denom
        defaultPacketData.denom = erc20AddressStr;
        packet.payloads[0].value = abi.encode(defaultPacketData);
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
        // reset data
        packet.payloads[0].value = abi.encode(defaultPacketData);

        // test invalid contract/denom
        defaultPacketData.denom = "invalid";
        packet.payloads[0].value = abi.encode(defaultPacketData);
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
        // reset denom
        defaultPacketData.denom = erc20AddressStr;
        packet.payloads[0].value = abi.encode(defaultPacketData);

        // test invalid sender
        defaultPacketData.sender = "invalid";
        packet.payloads[0].value = abi.encode(defaultPacketData);
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
        // reset sender
        defaultPacketData.sender = senderStr;
        packet.payloads[0].value = abi.encode(defaultPacketData);
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
        // reset data
        packet.payloads[0].value = abi.encode(defaultPacketData);

        // test invalid contract
        defaultPacketData.denom = "invalid";
        packet.payloads[0].value = abi.encode(defaultPacketData);
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
        // reset denom
        defaultPacketData.denom = erc20AddressStr;
        packet.payloads[0].value = abi.encode(defaultPacketData);

        // test invalid sender
        defaultPacketData.sender = "invalid";
        packet.payloads[0].value = abi.encode(defaultPacketData);
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
        // reset sender
        defaultPacketData.sender = senderStr;
        packet.payloads[0].value = abi.encode(defaultPacketData);
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

        ICS20Lib.FungibleTokenPacketData memory backPacketData = ICS20Lib.FungibleTokenPacketData({
            denom: receivedDenom,
            sender: senderStr,
            receiver: receiverStr,
            amount: defaultAmount,
            memo: "memo"
        });
        packet.payloads[0].value = abi.encode(backPacketData);
        packet.payloads[0].destPort = packet.payloads[0].sourcePort;
        packet.destChannel = packet.sourceChannel;
        packet.payloads[0].sourcePort = newSourcePort;
        packet.sourceChannel = newSourceChannel;

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
        assertEq(senderBalanceAfterReceive, defaultAmount);
        assertEq(contractBalanceAfterReceive, 0);
    }

    function test_success_onRecvPacketWithForeignBaseDenom() public {
        IICS26RouterMsgs.Packet memory packet = _getTestPacket();

        string memory foreignDenom = "uatom";

        senderStr = "cosmos1mhmwgrfrcrdex5gnr0vcqt90wknunsxej63feh";
        receiver = makeAddr("receiver_of_foreign_denom");
        receiverStr = Strings.toHexString(receiver);

        ICS20Lib.FungibleTokenPacketData memory receivePayload = ICS20Lib.FungibleTokenPacketData({
            denom: foreignDenom,
            amount: defaultAmount,
            sender: senderStr,
            receiver: receiverStr,
            memo: "memo"
        });
        packet.payloads[0].value = abi.encode(receivePayload);
        packet.payloads[0].destPort = ICS20Lib.DEFAULT_PORT_ID;
        packet.destChannel = "dest-channel";
        packet.payloads[0].sourcePort = ICS20Lib.DEFAULT_PORT_ID;
        packet.sourceChannel = "source-channel";

        string memory expectedFullDenomPath =
            string(abi.encodePacked(packet.payloads[0].destPort, "/", packet.destChannel, "/", foreignDenom));

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

          (address erc20Address,) = ics20Transfer.getReceiveERC20AddressAndSource(
            packet.payloads[0].sourcePort,
            packet.sourceChannel,
            packet.payloads[0].destPort,
            packet.destChannel,
            receivePayload
        );

        ICS20Lib.FungibleTokenPacketData memory packetData =
            abi.decode(packet.payloads[0].value, (ICS20Lib.FungibleTokenPacketData));
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

        ICS20Lib.FungibleTokenPacketData memory receivePayload = ICS20Lib.FungibleTokenPacketData({
            denom: foreignDenom,
            amount: defaultAmount,
            sender: senderStr,
            receiver: receiverStr,
            memo: "memo"
        });
        packet.payloads[0].value = abi.encode(receivePayload);
        packet.payloads[0].destPort = ICS20Lib.DEFAULT_PORT_ID;
        packet.destChannel = "dest-channel";
        packet.payloads[0].sourcePort = ICS20Lib.DEFAULT_PORT_ID;
        packet.sourceChannel = "source-channel";

        string memory expectedFullDenomPath =
            string(abi.encodePacked(packet.payloads[0].destPort, "/", packet.destChannel, "/", foreignDenom));

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

        (address erc20Address,) = ics20Transfer.getReceiveERC20AddressAndSource(
            packet.payloads[0].sourcePort,
            packet.sourceChannel,
            packet.payloads[0].destPort,
            packet.destChannel,
            receivePayload
        );

        ICS20Lib.FungibleTokenPacketData memory packetData =
            abi.decode(packet.payloads[0].value, (ICS20Lib.FungibleTokenPacketData));
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
        defaultPacketData.denom = ibcDenom;
        packet.payloads[0].value = abi.encode(defaultPacketData);

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
        packet.payloads[0].value = abi.encode(defaultPacketData);

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
        // reset data
        packet.payloads[0].value = abi.encode(defaultPacketData);

        // test invalid amount
        defaultPacketData.amount = 0;
        packet.payloads[0].value = abi.encode(defaultPacketData);
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
        // reset amount
        defaultPacketData.amount = defaultAmount;
        packet.payloads[0].value = abi.encode(defaultPacketData);

        // test receiver chain is source, but denom is not erc20 address
        string memory invalidErc20Denom =
            string(abi.encodePacked(packet.payloads[0].sourcePort, "/", packet.sourceChannel, "/invalid"));
        defaultPacketData.denom = invalidErc20Denom;
        packet.payloads[0].value = abi.encode(defaultPacketData);
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
        // reset denom
        defaultPacketData.denom = ibcDenom;
        packet.payloads[0].value = abi.encode(defaultPacketData);

        // test invalid receiver
        defaultPacketData.receiver = "invalid";
        packet.payloads[0].value = abi.encode(defaultPacketData);
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
        // reset receiver
        defaultPacketData.receiver = receiverStr;
        packet.payloads[0].value = abi.encode(defaultPacketData);
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
