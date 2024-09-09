// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.25 <0.9.0;

// solhint-disable custom-errors,max-line-length

import { Test } from "forge-std/Test.sol";
import { IICS26RouterMsgs } from "../src/msgs/IICS26RouterMsgs.sol";
import { IICS26Router } from "../src/interfaces/IICS26Router.sol";
import { IIBCAppCallbacks } from "../src/msgs/IIBCAppCallbacks.sol";
import { IICS20TransferMsgs } from "../src/msgs/IICS20TransferMsgs.sol";
import { IICS20TransferEvents } from "../src/events/IICS20TransferEvents.sol";
import { ICS20Transfer } from "../src/ICS20Transfer.sol";
import { TestERC20, MalfunctioningERC20 } from "./mocks/TestERC20.sol";
import { IBCERC20 } from "../src/utils/IBCERC20.sol";
import { IERC20Errors } from "@openzeppelin/contracts/interfaces/draft-IERC6093.sol";
import { ICS20Lib } from "../src/utils/ICS20Lib.sol";
import { IICS20Errors } from "../src/errors/IICS20Errors.sol";
import { Strings } from "@openzeppelin/contracts/utils/Strings.sol";
import { Vm } from "forge-std/Vm.sol";

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

    bytes public data;
    IICS26RouterMsgs.Packet public packet;
    ICS20Lib.PacketDataJSON public expectedDefaultSendPacketData;

    function setUp() public {
        ics20Transfer = new ICS20Transfer(address(this));
        erc20 = new TestERC20();

        sender = makeAddr("sender");

        erc20AddressStr = Strings.toHexString(address(erc20));
        senderStr = Strings.toHexString(sender);
        data = ICS20Lib.marshalJSON(erc20AddressStr, defaultAmount, senderStr, receiverStr, "memo");

        packet = IICS26RouterMsgs.Packet({
            sequence: 0,
            timeoutTimestamp: 0,
            sourcePort: "sourcePort",
            sourceChannel: "sourceChannel",
            destPort: "destinationPort",
            destChannel: "destinationChannel",
            version: ICS20Lib.ICS20_VERSION,
            data: data
        });

        expectedDefaultSendPacketData = ICS20Lib.PacketDataJSON({
            denom: erc20AddressStr,
            sender: senderStr,
            receiver: receiverStr,
            amount: defaultAmount,
            memo: "memo"
        });
    }

    function test_success_sendTransfer() public {
        IICS20TransferMsgs.SendTransferMsg memory msgSendTransfer = IICS20TransferMsgs.SendTransferMsg({
            denom: erc20AddressStr,
            amount: defaultAmount,
            receiver: receiverStr,
            sourceChannel: packet.sourceChannel,
            destPort: packet.sourcePort,
            timeoutTimestamp: uint64(block.timestamp + 1000),
            memo: "memo"
        });

        vm.mockCall(address(this), abi.encodeWithSelector(IICS26Router.sendPacket.selector), abi.encode(uint32(42)));
        vm.prank(sender);
        uint32 sequence = ics20Transfer.sendTransfer(msgSendTransfer);
        assertEq(sequence, 42);
    }

    function test_failure_sendTransfer() public {
        // just to make sure it doesn't accidentally revert on the router call
        vm.mockCall(address(this), abi.encodeWithSelector(IICS26Router.sendPacket.selector), abi.encode(uint32(42)));

        vm.startPrank(sender);

        IICS20TransferMsgs.SendTransferMsg memory msgSendTransfer = IICS20TransferMsgs.SendTransferMsg({
            denom: erc20AddressStr,
            amount: defaultAmount,
            receiver: receiverStr,
            sourceChannel: packet.sourceChannel,
            destPort: packet.sourcePort,
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

    function test_success_onSendPacket() public {
        erc20.mint(sender, defaultAmount);
        vm.prank(sender);
        erc20.approve(address(ics20Transfer), defaultAmount);

        uint256 senderBalanceBefore = erc20.balanceOf(sender);
        uint256 contractBalanceBefore = erc20.balanceOf(address(ics20Transfer));
        assertEq(senderBalanceBefore, defaultAmount);
        assertEq(contractBalanceBefore, 0);

        vm.expectEmit();
        emit IICS20TransferEvents.ICS20Transfer(expectedDefaultSendPacketData, address(erc20));
        ics20Transfer.onSendPacket(
            IIBCAppCallbacks.OnSendPacketCallback({ packet: packet, sender: address(ics20Transfer) })
        );

        uint256 senderBalanceAfter = erc20.balanceOf(sender);
        uint256 contractBalanceAfter = erc20.balanceOf(address(ics20Transfer));
        assertEq(senderBalanceAfter, 0);
        assertEq(contractBalanceAfter, defaultAmount);
    }

    /// @dev to document the behaviour of the contract when calling onSendPacket directly
    function test_success_onSendPacketWithLargeAmount() public {
        uint256 largeAmount = 1_000_000_000_000_000_001_000_000_000_000;

        erc20.mint(sender, largeAmount);
        vm.prank(sender);
        erc20.approve(address(ics20Transfer), largeAmount);

        uint256 senderBalanceBefore = erc20.balanceOf(sender);
        uint256 contractBalanceBefore = erc20.balanceOf(address(ics20Transfer));
        assertEq(senderBalanceBefore, largeAmount);
        assertEq(contractBalanceBefore, 0);

        data = ICS20Lib.marshalJSON(erc20AddressStr, largeAmount, senderStr, receiverStr, "memo");
        packet.data = data;
        expectedDefaultSendPacketData.amount = largeAmount;

        vm.expectEmit();
        emit IICS20TransferEvents.ICS20Transfer(expectedDefaultSendPacketData, address(erc20));
        ics20Transfer.onSendPacket(
            IIBCAppCallbacks.OnSendPacketCallback({ packet: packet, sender: address(ics20Transfer) })
        );

        assertEq(erc20.balanceOf(sender), 0);
        assertEq(erc20.balanceOf(address(ics20Transfer)), largeAmount);
    }

    function test_failure_onSendPacket() public {
        // test missing approval
        vm.expectRevert(
            abi.encodeWithSelector(
                IERC20Errors.ERC20InsufficientAllowance.selector, address(ics20Transfer), 0, defaultAmount
            )
        );
        ics20Transfer.onSendPacket(
            IIBCAppCallbacks.OnSendPacketCallback({ packet: packet, sender: address(ics20Transfer) })
        );

        // test insufficient balance
        vm.prank(sender);
        erc20.approve(address(ics20Transfer), defaultAmount);
        vm.expectRevert(
            abi.encodeWithSelector(IERC20Errors.ERC20InsufficientBalance.selector, sender, 0, defaultAmount)
        );
        ics20Transfer.onSendPacket(
            IIBCAppCallbacks.OnSendPacketCallback({ packet: packet, sender: address(ics20Transfer) })
        );

        // test invalid amount
        data = ICS20Lib.marshalJSON(erc20AddressStr, 0, senderStr, receiverStr, "memo");
        packet.data = data;
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20InvalidAmount.selector, 0));
        ics20Transfer.onSendPacket(
            IIBCAppCallbacks.OnSendPacketCallback({ packet: packet, sender: address(ics20Transfer) })
        );

        // test invalid data
        data = bytes("invalid");
        packet.data = data;
        vm.expectRevert(bytes(""));
        ics20Transfer.onSendPacket(
            IIBCAppCallbacks.OnSendPacketCallback({ packet: packet, sender: address(ics20Transfer) })
        );

        // test invalid sender
        data = ICS20Lib.marshalJSON(erc20AddressStr, defaultAmount, "invalid", receiverStr, "memo");
        packet.data = data;
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20InvalidAddress.selector, "invalid"));
        ics20Transfer.onSendPacket(
            IIBCAppCallbacks.OnSendPacketCallback({ packet: packet, sender: address(ics20Transfer) })
        );

        // test msg sender is the token sender (i.e. not ics20Transfer)
        data = ICS20Lib.marshalJSON(erc20AddressStr, defaultAmount, senderStr, receiverStr, "memo");
        packet.data = data;
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20UnauthorizedPacketSender.selector, sender));
        ics20Transfer.onSendPacket(IIBCAppCallbacks.OnSendPacketCallback({ packet: packet, sender: sender }));
        // test msg sender is someone else entirely (i.e. not ics20Transfer)
        address someoneElse = makeAddr("someoneElse");
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20UnauthorizedPacketSender.selector, someoneElse));
        ics20Transfer.onSendPacket(IIBCAppCallbacks.OnSendPacketCallback({ packet: packet, sender: someoneElse }));

        // test invalid token contract
        data = ICS20Lib.marshalJSON("invalid", defaultAmount, senderStr, receiverStr, "memo");
        packet.data = data;
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20InvalidAddress.selector, "invalid"));
        ics20Transfer.onSendPacket(
            IIBCAppCallbacks.OnSendPacketCallback({ packet: packet, sender: address(ics20Transfer) })
        );

        // test invalid version
        packet.version = "invalid";
        vm.expectRevert(
            abi.encodeWithSelector(IICS20Errors.ICS20UnexpectedVersion.selector, ICS20Lib.ICS20_VERSION, "invalid")
        );
        ics20Transfer.onSendPacket(
            IIBCAppCallbacks.OnSendPacketCallback({ packet: packet, sender: address(ics20Transfer) })
        );
        // Reset version
        packet.version = ICS20Lib.ICS20_VERSION;

        // test malfunctioning transfer
        MalfunctioningERC20 malfunctioningERC20 = new MalfunctioningERC20();
        malfunctioningERC20.mint(sender, defaultAmount);
        malfunctioningERC20.setMalfunction(true); // Turn on the malfunctioning behaviour (no update)
        vm.prank(sender);
        malfunctioningERC20.approve(address(ics20Transfer), defaultAmount);
        string memory malfuncERC20AddressStr = Strings.toHexString(address(malfunctioningERC20));
        data = ICS20Lib.marshalJSON(malfuncERC20AddressStr, defaultAmount, senderStr, receiverStr, "memo");
        packet.data = data;
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20UnexpectedERC20Balance.selector, defaultAmount, 0));
        ics20Transfer.onSendPacket(
            IIBCAppCallbacks.OnSendPacketCallback({ packet: packet, sender: address(ics20Transfer) })
        );
    }

    function test_success_onAcknowledgementPacketWithSuccessAck() public {
        erc20.mint(sender, defaultAmount);
        vm.prank(sender);
        erc20.approve(address(ics20Transfer), defaultAmount);

        uint256 senderBalanceBefore = erc20.balanceOf(sender);
        uint256 contractBalanceBefore = erc20.balanceOf(address(ics20Transfer));
        assertEq(senderBalanceBefore, defaultAmount);
        assertEq(contractBalanceBefore, 0);

        vm.expectEmit();
        emit IICS20TransferEvents.ICS20Transfer(expectedDefaultSendPacketData, address(erc20));
        ics20Transfer.onSendPacket(
            IIBCAppCallbacks.OnSendPacketCallback({ packet: packet, sender: address(ics20Transfer) })
        );

        uint256 senderBalanceAfterSend = erc20.balanceOf(sender);
        uint256 contractBalanceAfterSend = erc20.balanceOf(address(ics20Transfer));
        assertEq(senderBalanceAfterSend, 0);
        assertEq(contractBalanceAfterSend, defaultAmount);

        vm.expectEmit();
        emit IICS20TransferEvents.ICS20Acknowledgement(
            expectedDefaultSendPacketData, ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON
        );
        ics20Transfer.onAcknowledgementPacket(
            IIBCAppCallbacks.OnAcknowledgementPacketCallback({
                packet: packet,
                acknowledgement: ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON,
                relayer: makeAddr("relayer")
            })
        );

        // Nothing should change
        uint256 senderBalanceAfterAck = erc20.balanceOf(sender);
        uint256 contractBalanceAfterAck = erc20.balanceOf(address(ics20Transfer));
        assertEq(senderBalanceAfterAck, 0);
        assertEq(contractBalanceAfterAck, defaultAmount);
    }

    function test_success_onAcknowledgementPacketWithFailedAck() public {
        erc20.mint(sender, defaultAmount);
        vm.prank(sender);
        erc20.approve(address(ics20Transfer), defaultAmount);

        uint256 senderBalanceBefore = erc20.balanceOf(sender);
        uint256 contractBalanceBefore = erc20.balanceOf(address(ics20Transfer));
        assertEq(senderBalanceBefore, defaultAmount);
        assertEq(contractBalanceBefore, 0);

        vm.expectEmit();
        emit IICS20TransferEvents.ICS20Transfer(expectedDefaultSendPacketData, address(erc20));
        ics20Transfer.onSendPacket(
            IIBCAppCallbacks.OnSendPacketCallback({ packet: packet, sender: address(ics20Transfer) })
        );

        uint256 senderBalanceAfterSend = erc20.balanceOf(sender);
        uint256 contractBalanceAfterSend = erc20.balanceOf(address(ics20Transfer));
        assertEq(senderBalanceAfterSend, 0);
        assertEq(contractBalanceAfterSend, defaultAmount);

        vm.expectEmit();
        emit IICS20TransferEvents.ICS20Acknowledgement(
            expectedDefaultSendPacketData, ICS20Lib.FAILED_ACKNOWLEDGEMENT_JSON
        );
        ics20Transfer.onAcknowledgementPacket(
            IIBCAppCallbacks.OnAcknowledgementPacketCallback({
                packet: packet,
                acknowledgement: ICS20Lib.FAILED_ACKNOWLEDGEMENT_JSON,
                relayer: makeAddr("relayer")
            })
        );

        // transfer should be reverted
        uint256 senderBalanceAfterAck = erc20.balanceOf(sender);
        uint256 contractBalanceAfterAck = erc20.balanceOf(address(ics20Transfer));
        assertEq(senderBalanceAfterAck, defaultAmount);
        assertEq(contractBalanceAfterAck, 0);
    }

    function test_failure_onAcknowledgementPacket() public {
        // test invalid data
        data = bytes("invalid");
        packet.data = data;
        vm.expectRevert(bytes(""));
        ics20Transfer.onAcknowledgementPacket(
            IIBCAppCallbacks.OnAcknowledgementPacketCallback({
                packet: packet,
                acknowledgement: ICS20Lib.FAILED_ACKNOWLEDGEMENT_JSON,
                relayer: makeAddr("relayer")
            })
        );

        // test invalid contract
        data = ICS20Lib.marshalJSON("invalid", defaultAmount, senderStr, receiverStr, "memo");
        packet.data = data;
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20InvalidAddress.selector, "invalid"));
        ics20Transfer.onAcknowledgementPacket(
            IIBCAppCallbacks.OnAcknowledgementPacketCallback({
                packet: packet,
                acknowledgement: ICS20Lib.FAILED_ACKNOWLEDGEMENT_JSON,
                relayer: makeAddr("relayer")
            })
        );

        // test invalid sender
        data = ICS20Lib.marshalJSON(erc20AddressStr, defaultAmount, "invalid", receiverStr, "memo");
        packet.data = data;
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20InvalidAddress.selector, "invalid"));
        ics20Transfer.onAcknowledgementPacket(
            IIBCAppCallbacks.OnAcknowledgementPacketCallback({
                packet: packet,
                acknowledgement: ICS20Lib.FAILED_ACKNOWLEDGEMENT_JSON,
                relayer: makeAddr("relayer")
            })
        );
    }

    function test_success_onTimeoutPacket() public {
        erc20.mint(sender, defaultAmount);
        vm.prank(sender);
        erc20.approve(address(ics20Transfer), defaultAmount);

        uint256 senderBalanceBefore = erc20.balanceOf(sender);
        uint256 contractBalanceBefore = erc20.balanceOf(address(ics20Transfer));
        assertEq(senderBalanceBefore, defaultAmount);
        assertEq(contractBalanceBefore, 0);

        vm.expectEmit();
        emit IICS20TransferEvents.ICS20Transfer(expectedDefaultSendPacketData, address(erc20));
        ics20Transfer.onSendPacket(
            IIBCAppCallbacks.OnSendPacketCallback({ packet: packet, sender: address(ics20Transfer) })
        );

        uint256 senderBalanceAfterSend = erc20.balanceOf(sender);
        uint256 contractBalanceAfterSend = erc20.balanceOf(address(ics20Transfer));
        assertEq(senderBalanceAfterSend, 0);
        assertEq(contractBalanceAfterSend, defaultAmount);

        vm.expectEmit();
        emit IICS20TransferEvents.ICS20Timeout(expectedDefaultSendPacketData);
        ics20Transfer.onTimeoutPacket(
            IIBCAppCallbacks.OnTimeoutPacketCallback({ packet: packet, relayer: makeAddr("relayer") })
        );

        // transfer should be reverted
        uint256 senderBalanceAfterAck = erc20.balanceOf(sender);
        uint256 contractBalanceAfterAck = erc20.balanceOf(address(ics20Transfer));
        assertEq(senderBalanceAfterAck, defaultAmount);
        assertEq(contractBalanceAfterAck, 0);
    }

    function test_failure_onTimeoutPacket() public {
        // test invalid data
        data = bytes("invalid");
        packet.data = data;
        vm.expectRevert(bytes(""));
        ics20Transfer.onTimeoutPacket(
            IIBCAppCallbacks.OnTimeoutPacketCallback({ packet: packet, relayer: makeAddr("relayer") })
        );

        // test invalid contract
        data = ICS20Lib.marshalJSON("invalid", defaultAmount, senderStr, receiverStr, "memo");
        packet.data = data;
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20InvalidAddress.selector, "invalid"));
        ics20Transfer.onTimeoutPacket(
            IIBCAppCallbacks.OnTimeoutPacketCallback({ packet: packet, relayer: makeAddr("relayer") })
        );

        // test invalid sender
        data = ICS20Lib.marshalJSON(erc20AddressStr, defaultAmount, "invalid", receiverStr, "memo");
        packet.data = data;
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20InvalidAddress.selector, "invalid"));
        ics20Transfer.onTimeoutPacket(
            IIBCAppCallbacks.OnTimeoutPacketCallback({ packet: packet, relayer: makeAddr("relayer") })
        );
    }

    function test_success_onRecvPacketWithSourceDenom() public {
        erc20.mint(sender, defaultAmount);
        vm.prank(sender);
        erc20.approve(address(ics20Transfer), defaultAmount);

        uint256 senderBalanceBefore = erc20.balanceOf(sender);
        uint256 contractBalanceBefore = erc20.balanceOf(address(ics20Transfer));
        assertEq(senderBalanceBefore, defaultAmount);
        assertEq(contractBalanceBefore, 0);

        vm.expectEmit();
        emit IICS20TransferEvents.ICS20Transfer(expectedDefaultSendPacketData, address(erc20));
        ics20Transfer.onSendPacket(
            IIBCAppCallbacks.OnSendPacketCallback({ packet: packet, sender: address(ics20Transfer) })
        );

        uint256 senderBalanceAfterSend = erc20.balanceOf(sender);
        uint256 contractBalanceAfterSend = erc20.balanceOf(address(ics20Transfer));
        assertEq(senderBalanceAfterSend, 0);
        assertEq(contractBalanceAfterSend, defaultAmount);

        // Send back (onRecv)
        string memory newSourcePort = packet.destPort;
        string memory newSourceChannel = packet.destChannel;
        string memory receivedDenom =
            string(abi.encodePacked(newSourcePort, "/", newSourceChannel, "/", erc20AddressStr));

        {
            string memory tmpSenderStr = senderStr;
            senderStr = receiverStr;
            receiverStr = tmpSenderStr;
        }
        packet.data = ICS20Lib.marshalJSON(receivedDenom, defaultSdkCoinAmount, senderStr, receiverStr, "memo");
        packet.destPort = packet.sourcePort;
        packet.destChannel = packet.sourceChannel;
        packet.sourcePort = newSourcePort;
        packet.sourceChannel = newSourceChannel;

        vm.expectEmit();
        emit IICS20TransferEvents.ICS20ReceiveTransfer(
            ICS20Lib.PacketDataJSON({
                denom: receivedDenom,
                sender: senderStr,
                receiver: receiverStr,
                amount: defaultSdkCoinAmount,
                memo: "memo"
            }),
            address(erc20)
        );
        bytes memory ack = ics20Transfer.onRecvPacket(
            IIBCAppCallbacks.OnRecvPacketCallback({ packet: packet, relayer: makeAddr("relayer") })
        );
        assertEq(ack, ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON);

        // the tokens should have been transferred back again
        uint256 senderBalanceAfterReceive = erc20.balanceOf(sender);
        uint256 contractBalanceAfterReceive = erc20.balanceOf(address(ics20Transfer));
        assertEq(senderBalanceAfterReceive, defaultSdkCoinAmount);
        assertEq(contractBalanceAfterReceive, defaultAmount - defaultSdkCoinAmount);
    }

    function test_success_onRecvPacketWithForeignBaseDenom() public {
        string memory foreignDenom = "uatom";

        senderStr = "cosmos1mhmwgrfrcrdex5gnr0vcqt90wknunsxej63feh";
        receiver = makeAddr("receiver_of_foreign_denom");
        receiverStr = Strings.toHexString(receiver);
        bytes memory receiveData = ICS20Lib.marshalJSON(foreignDenom, defaultAmount, senderStr, receiverStr, "memo");
        packet.data = receiveData;
        packet.destPort = "transfer";
        packet.destChannel = "dest-channel";
        packet.sourcePort = "transfer";
        packet.sourceChannel = "source-channel";

        string memory expectedFullDenomPath =
            string(abi.encodePacked(packet.destPort, "/", packet.destChannel, "/", foreignDenom));

        vm.expectEmit(true, true, true, false); // Not checking data because we don't know the address yet
        ICS20Lib.PacketDataJSON memory packetData;
        address erc20Address;
        emit IICS20TransferEvents.ICS20ReceiveTransfer(packetData, erc20Address); // we check these values later
        vm.recordLogs();
        bytes memory ack = ics20Transfer.onRecvPacket(
            IIBCAppCallbacks.OnRecvPacketCallback({ packet: packet, relayer: makeAddr("relayer") })
        );
        assertEq(ack, ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON);

        // find and extract data from the ICS20ReceiveTransfer event
        Vm.Log[] memory entries = vm.getRecordedLogs();
        assertEq(entries.length, 4);
        Vm.Log memory receiveTransferLog = entries[3];
        assertEq(receiveTransferLog.topics[0], IICS20TransferEvents.ICS20ReceiveTransfer.selector);

        (packetData, erc20Address) = abi.decode(receiveTransferLog.data, (ICS20Lib.PacketDataJSON, address));
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
        string memory foreignDenom = "transfer/channel-42/uatom";

        senderStr = "cosmos1mhmwgrfrcrdex5gnr0vcqt90wknunsxej63feh";
        receiver = makeAddr("receiver_of_foreign_denom");
        receiverStr = Strings.toHexString(receiver);
        packet.data = ICS20Lib.marshalJSON(foreignDenom, defaultAmount, senderStr, receiverStr, "memo");
        packet.destPort = "transfer";
        packet.destChannel = "dest-channel";
        packet.sourcePort = "transfer";
        packet.sourceChannel = "source-channel";

        string memory expectedFullDenomPath =
            string(abi.encodePacked(packet.destPort, "/", packet.destChannel, "/", foreignDenom));

        vm.expectEmit(true, true, true, false); // Not checking data because we don't know the address yet
        ICS20Lib.PacketDataJSON memory packetData;
        address erc20Address;
        emit IICS20TransferEvents.ICS20ReceiveTransfer(packetData, erc20Address); // we check these values later
        vm.recordLogs();
        bytes memory ack = ics20Transfer.onRecvPacket(
            IIBCAppCallbacks.OnRecvPacketCallback({ packet: packet, relayer: makeAddr("relayer") })
        );
        assertEq(ack, ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON);

        // find and extract data from the ICS20ReceiveTransfer event
        Vm.Log[] memory entries = vm.getRecordedLogs();
        assertEq(entries.length, 4);
        Vm.Log memory receiveTransferLog = entries[3];
        assertEq(receiveTransferLog.topics[0], IICS20TransferEvents.ICS20ReceiveTransfer.selector);

        (packetData, erc20Address) = abi.decode(receiveTransferLog.data, (ICS20Lib.PacketDataJSON, address));
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
        string memory ibcDenom =
            string(abi.encodePacked(packet.sourcePort, "/", packet.sourceChannel, "/", erc20AddressStr));
        packet.data = ICS20Lib.marshalJSON(ibcDenom, defaultAmount, receiverStr, senderStr, "memo");

        // test invalid version
        packet.version = "invalid";
        bytes memory ack = ics20Transfer.onRecvPacket(
            IIBCAppCallbacks.OnRecvPacketCallback({ packet: packet, relayer: makeAddr("relayer") })
        );
        assertEq(string(ack), "{\"error\":\"unexpected version: invalid\"}");
        // Reset version
        packet.version = ICS20Lib.ICS20_VERSION;

        // test invalid data
        data = bytes("invalid");
        packet.data = data;
        vm.expectRevert(bytes(""));
        ics20Transfer.onRecvPacket(
            IIBCAppCallbacks.OnRecvPacketCallback({ packet: packet, relayer: makeAddr("relayer") })
        );

        // test invalid amount
        data = ICS20Lib.marshalJSON(ibcDenom, 0, receiverStr, senderStr, "memo");
        packet.data = data;
        ack = ics20Transfer.onRecvPacket(
            IIBCAppCallbacks.OnRecvPacketCallback({ packet: packet, relayer: makeAddr("relayer") })
        );
        assertEq(string(ack), "{\"error\":\"invalid amount: 0\"}");

        // test receiver chain is source, but denom is not erc20 address
        string memory invalidErc20Denom =
            string(abi.encodePacked(packet.sourcePort, "/", packet.sourceChannel, "/invalid"));
        data = ICS20Lib.marshalJSON(invalidErc20Denom, defaultAmount, receiverStr, senderStr, "memo");
        packet.data = data;
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20InvalidAddress.selector, "invalid"));
        ics20Transfer.onRecvPacket(
            IIBCAppCallbacks.OnRecvPacketCallback({ packet: packet, relayer: makeAddr("relayer") })
        );

        // test invalid receiver
        data = ICS20Lib.marshalJSON(ibcDenom, defaultAmount, receiverStr, "invalid", "memo");
        packet.data = data;
        ack = ics20Transfer.onRecvPacket(
            IIBCAppCallbacks.OnRecvPacketCallback({ packet: packet, relayer: makeAddr("relayer") })
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
        packet.data = wrongOrderJSON;
        vm.expectRevert(
            abi.encodeWithSelector(
                IICS20Errors.ICS20JSONUnexpectedBytes.selector, 0, bytes32("{\"denom\":\""), bytes32("{\"amount\":")
            )
        );
        ics20Transfer.onRecvPacket(
            IIBCAppCallbacks.OnRecvPacketCallback({ packet: packet, relayer: makeAddr("relayer") })
        );
    }
}
