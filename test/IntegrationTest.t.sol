// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.25 <0.9.0;

// solhint-disable custom-errors,max-line-length,max-states-count

import { Test } from "forge-std/Test.sol";
import { IICS02Client } from "../src/interfaces/IICS02Client.sol";
import { IICS02ClientMsgs } from "../src/msgs/IICS02ClientMsgs.sol";
import { ICS20Transfer } from "../src/ICS20Transfer.sol";
import { IICS20Transfer } from "../src/interfaces/IICS20Transfer.sol";
import { IICS20TransferMsgs } from "../src/msgs/IICS20TransferMsgs.sol";
import { TestERC20 } from "./TestERC20.sol";
import { IBCERC20 } from "../src/utils/IBCERC20.sol";
import { ICS02Client } from "../src/ICS02Client.sol";
import { IICS26Router } from "../src/ICS26Router.sol";
import { IICS26RouterErrors } from "../src/errors/IICS26RouterErrors.sol";
import { ICS26Router } from "../src/ICS26Router.sol";
import { IICS26RouterMsgs } from "../src/msgs/IICS26RouterMsgs.sol";
import { DummyLightClient } from "./DummyLightClient.sol";
import { ILightClientMsgs } from "../src/msgs/ILightClientMsgs.sol";
import { ICS20Lib } from "../src/utils/ICS20Lib.sol";
import { ICS24Host } from "../src/utils/ICS24Host.sol";
import { Strings } from "@openzeppelin/contracts/utils/Strings.sol";
import { Vm } from "forge-std/Vm.sol";

contract IntegrationTest is Test {
    IICS02Client public ics02Client;
    ICS26Router public ics26Router;
    DummyLightClient public lightClient;
    string public clientIdentifier;
    ICS20Transfer public ics20Transfer;
    string public ics20AddressStr;
    TestERC20 public erc20;
    string public erc20AddressStr;
    string public counterpartyClient = "42-dummy-01";

    uint256 public defaultAmount = 1000;
    address public sender;
    string public senderStr;
    string public receiverStr = "someReceiver";
    bytes public data;
    IICS26RouterMsgs.MsgSendPacket public msgSendPacket;
    ICS20Lib.PacketDataJSON public expectedDefaultSendPacketData;

    function setUp() public {
        ics02Client = new ICS02Client(address(this));
        ics26Router = new ICS26Router(address(ics02Client), address(this));
        lightClient = new DummyLightClient(ILightClientMsgs.UpdateResult.Update, 0, false);
        ics20Transfer = new ICS20Transfer(address(ics26Router));
        erc20 = new TestERC20();
        erc20AddressStr = Strings.toHexString(address(erc20));

        clientIdentifier = ics02Client.addClient(
            "07-tendermint", IICS02ClientMsgs.CounterpartyInfo(counterpartyClient), address(lightClient)
        );
        ics20AddressStr = Strings.toHexString(address(ics20Transfer));

        vm.expectEmit();
        emit IICS26Router.IBCAppAdded("transfer", address(ics20Transfer));
        ics26Router.addIBCApp("transfer", address(ics20Transfer));

        assertEq(address(ics20Transfer), address(ics26Router.getIBCApp("transfer")));

        sender = makeAddr("sender");
        senderStr = Strings.toHexString(sender);
        data = ICS20Lib.marshalJSON(erc20AddressStr, defaultAmount, senderStr, receiverStr, "memo");
        msgSendPacket = IICS26RouterMsgs.MsgSendPacket({
            sourcePort: "transfer",
            sourceChannel: clientIdentifier,
            destPort: "transfer",
            data: data,
            timeoutTimestamp: uint64(block.timestamp + 1000),
            version: ICS20Lib.ICS20_VERSION
        });

        expectedDefaultSendPacketData = ICS20Lib.PacketDataJSON({
            denom: erc20AddressStr,
            sender: senderStr,
            receiver: receiverStr,
            amount: defaultAmount,
            memo: "memo"
        });
    }

    function test_success_sendICS20PacketDirectlyFromRouter() public {
        erc20.mint(sender, defaultAmount);
        vm.startPrank(sender);
        erc20.approve(address(ics20Transfer), defaultAmount);

        uint256 senderBalanceBefore = erc20.balanceOf(sender);
        uint256 contractBalanceBefore = erc20.balanceOf(address(ics20Transfer));
        assertEq(senderBalanceBefore, defaultAmount);
        assertEq(contractBalanceBefore, 0);

        vm.expectEmit();
        emit IICS20Transfer.ICS20Transfer(expectedDefaultSendPacketData, address(erc20));
        uint32 sequence = ics26Router.sendPacket(msgSendPacket);
        assertEq(sequence, 1);

        bytes32 path =
            ICS24Host.packetCommitmentKeyCalldata(msgSendPacket.sourcePort, msgSendPacket.sourceChannel, sequence);
        bytes32 storedCommitment = ics26Router.getCommitment(path);
        IICS26RouterMsgs.Packet memory packet = _getPacket(msgSendPacket, sequence);
        assertEq(storedCommitment, ICS24Host.packetCommitmentBytes32(packet));

        IICS26RouterMsgs.MsgAckPacket memory ackMsg = IICS26RouterMsgs.MsgAckPacket({
            packet: packet,
            acknowledgement: ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON,
            proofAcked: bytes("doesntmatter"), // dummy client will accept
            proofHeight: IICS02ClientMsgs.Height({ revisionNumber: 1, revisionHeight: 42 }) // dummy client will accept
         });
        vm.expectEmit();
        emit IICS20Transfer.ICS20Acknowledgement(
            expectedDefaultSendPacketData, ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON
        );
        ics26Router.ackPacket(ackMsg);
        // commitment should be deleted
        storedCommitment = ics26Router.getCommitment(path);
        assertEq(storedCommitment, 0);

        uint256 senderBalanceAfter = erc20.balanceOf(sender);
        uint256 contractBalanceAfter = erc20.balanceOf(address(ics20Transfer));
        assertEq(senderBalanceAfter, 0);
        assertEq(contractBalanceAfter, defaultAmount);
    }

    function test_success_sendICS20PacketFromICS20Contract() public {
        IICS26RouterMsgs.Packet memory packet = _sendICS20Transfer();

        IICS26RouterMsgs.MsgAckPacket memory ackMsg = IICS26RouterMsgs.MsgAckPacket({
            packet: packet,
            acknowledgement: ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON,
            proofAcked: bytes("doesntmatter"), // dummy client will accept
            proofHeight: IICS02ClientMsgs.Height({ revisionNumber: 1, revisionHeight: 42 }) // dummy client will accept
         });
        vm.expectEmit();
        emit IICS20Transfer.ICS20Acknowledgement(
            expectedDefaultSendPacketData, ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON
        );
        ics26Router.ackPacket(ackMsg);
        // commitment should be deleted
        bytes32 path = ICS24Host.packetCommitmentKeyCalldata(
            msgSendPacket.sourcePort, msgSendPacket.sourceChannel, packet.sequence
        );
        bytes32 storedCommitment = ics26Router.getCommitment(path);
        assertEq(storedCommitment, 0);

        uint256 senderBalanceAfter = erc20.balanceOf(sender);
        uint256 contractBalanceAfter = erc20.balanceOf(address(ics20Transfer));
        assertEq(senderBalanceAfter, 0);
        assertEq(contractBalanceAfter, defaultAmount);
    }

    function test_success_failedCounterpartyAckForICS20Packet() public {
        IICS26RouterMsgs.Packet memory packet = _sendICS20Transfer();

        IICS26RouterMsgs.MsgAckPacket memory ackMsg = IICS26RouterMsgs.MsgAckPacket({
            packet: packet,
            acknowledgement: ICS20Lib.FAILED_ACKNOWLEDGEMENT_JSON,
            proofAcked: bytes("doesntmatter"), // dummy client will accept
            proofHeight: IICS02ClientMsgs.Height({ revisionNumber: 1, revisionHeight: 42 }) // dummy client will accept
         });
        vm.expectEmit();
        emit IICS20Transfer.ICS20Acknowledgement(expectedDefaultSendPacketData, ICS20Lib.FAILED_ACKNOWLEDGEMENT_JSON);
        ics26Router.ackPacket(ackMsg);
        // commitment should be deleted
        bytes32 path = ICS24Host.packetCommitmentKeyCalldata(
            msgSendPacket.sourcePort, msgSendPacket.sourceChannel, packet.sequence
        );
        bytes32 storedCommitment = ics26Router.getCommitment(path);
        assertEq(storedCommitment, 0);

        // transfer should be reverted
        uint256 senderBalanceAfterAck = erc20.balanceOf(sender);
        uint256 contractBalanceAfterAck = erc20.balanceOf(address(ics20Transfer));
        assertEq(senderBalanceAfterAck, defaultAmount);
        assertEq(contractBalanceAfterAck, 0);
    }

    function test_success_timeoutICS20Packet() public {
        IICS26RouterMsgs.Packet memory packet = _sendICS20Transfer();

        // make light client return timestamp that is after our timeout
        lightClient.setMembershipResult(msgSendPacket.timeoutTimestamp + 1, false);

        IICS26RouterMsgs.MsgTimeoutPacket memory timeoutMsg = IICS26RouterMsgs.MsgTimeoutPacket({
            packet: packet,
            proofTimeout: bytes("doesntmatter"), // dummy client will accept
            proofHeight: IICS02ClientMsgs.Height({ revisionNumber: 1, revisionHeight: 42 }) // dummy client will accept
         });
        vm.expectEmit();
        emit IICS20Transfer.ICS20Timeout(expectedDefaultSendPacketData);
        ics26Router.timeoutPacket(timeoutMsg);
        // commitment should be deleted
        bytes32 path = ICS24Host.packetCommitmentKeyCalldata(
            msgSendPacket.sourcePort, msgSendPacket.sourceChannel, packet.sequence
        );
        bytes32 storedCommitment = ics26Router.getCommitment(path);
        assertEq(storedCommitment, 0);

        // transfer should be reverted
        uint256 senderBalanceAfterTimeout = erc20.balanceOf(sender);
        uint256 contractBalanceAfterTimeout = erc20.balanceOf(address(ics20Transfer));
        assertEq(senderBalanceAfterTimeout, defaultAmount);
        assertEq(contractBalanceAfterTimeout, 0);
    }

    function test_success_receiveICS20PacketWithSourceDenom() public {
        IICS26RouterMsgs.Packet memory packet = _sendICS20Transfer();

        IICS26RouterMsgs.MsgAckPacket memory ackMsg = IICS26RouterMsgs.MsgAckPacket({
            packet: packet,
            acknowledgement: ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON,
            proofAcked: bytes("doesntmatter"), // dummy client will accept
            proofHeight: IICS02ClientMsgs.Height({ revisionNumber: 1, revisionHeight: 42 }) // dummy client will accept
         });
        vm.expectEmit();
        emit IICS20Transfer.ICS20Acknowledgement(
            expectedDefaultSendPacketData, ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON
        );
        ics26Router.ackPacket(ackMsg);

        // commitment should be deleted
        bytes32 path = ICS24Host.packetCommitmentKeyCalldata(
            msgSendPacket.sourcePort, msgSendPacket.sourceChannel, packet.sequence
        );
        bytes32 storedCommitment = ics26Router.getCommitment(path);
        assertEq(storedCommitment, 0);

        uint256 senderBalanceAfterSend = erc20.balanceOf(sender);
        uint256 contractBalanceAfterSend = erc20.balanceOf(address(ics20Transfer));
        assertEq(senderBalanceAfterSend, 0);
        assertEq(contractBalanceAfterSend, defaultAmount);

        // Send back
        string memory backSender = "cosmos1mhmwgrfrcrdex5gnr0vcqt90wknunsxej63feh";
        address backReceiver = sender;
        string memory backReceiverStr = senderStr;
        string memory receivedDenom = string(abi.encodePacked("transfer/", counterpartyClient, "/", erc20AddressStr));
        data = ICS20Lib.marshalJSON(receivedDenom, defaultAmount, backSender, backReceiverStr, "backmemo");

        // For the packet back we pretend this is ibc-go and that the timeout is in nanoseconds
        packet = IICS26RouterMsgs.Packet({
            sequence: 1,
            timeoutTimestamp: packet.timeoutTimestamp + 1000,
            sourcePort: "transfer",
            sourceChannel: counterpartyClient,
            destPort: "transfer",
            destChannel: clientIdentifier,
            version: ICS20Lib.ICS20_VERSION,
            data: data
        });
        vm.expectEmit();
        emit IICS20Transfer.ICS20ReceiveTransfer(
            ICS20Lib.PacketDataJSON({
                denom: receivedDenom,
                sender: backSender,
                receiver: backReceiverStr,
                amount: defaultAmount,
                memo: "backmemo"
            }),
            address(erc20)
        );
        vm.expectEmit();
        emit IICS26Router.WriteAcknowledgement(packet, ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON);
        vm.expectEmit();
        emit IICS26Router.RecvPacket(packet);

        ics26Router.recvPacket(
            IICS26RouterMsgs.MsgRecvPacket({
                packet: packet,
                proofCommitment: bytes("doesntmatter"), // dummy client will accept
                proofHeight: IICS02ClientMsgs.Height({ revisionNumber: 1, revisionHeight: 42 }) // will accept
             })
        );

        // Check balances are updated as expected
        uint256 backReceiverBalance = erc20.balanceOf(backReceiver);
        uint256 contractBalanceAfterRecv = erc20.balanceOf(address(ics20Transfer));
        assertEq(backReceiverBalance, defaultAmount);
        assertEq(contractBalanceAfterRecv, 0);

        // Check that the ack is written
        bytes32 ackPath =
            ICS24Host.packetAcknowledgementCommitmentKeyCalldata(packet.destPort, packet.destChannel, packet.sequence);
        bytes32 storedAck = ics26Router.getCommitment(ackPath);
        assertEq(storedAck, ICS24Host.packetAcknowledgementCommitmentBytes32(ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON));
    }

    function test_success_receiveICS20PacketWithForeignBaseDenom() public {
        string memory foreignDenom = "uatom";

        string memory senderAddrStr = "cosmos1mhmwgrfrcrdex5gnr0vcqt90wknunsxej63feh";
        address receiverAddr = makeAddr("receiver_of_foreign_denom");
        string memory receiverAddrStr = Strings.toHexString(receiverAddr);

        // For the packet back we pretend this is ibc-go and that the timeout is in nanoseconds
        IICS26RouterMsgs.Packet memory receivePacket = IICS26RouterMsgs.Packet({
            sequence: 1,
            timeoutTimestamp: uint64(block.timestamp + 1000),
            sourcePort: "transfer",
            sourceChannel: counterpartyClient,
            destPort: "transfer",
            destChannel: clientIdentifier,
            version: ICS20Lib.ICS20_VERSION,
            data: ICS20Lib.marshalJSON(foreignDenom, defaultAmount, senderAddrStr, receiverAddrStr, "memo")
        });

        string memory expectedFullDenomPath =
            string(abi.encodePacked(receivePacket.destPort, "/", receivePacket.destChannel, "/", foreignDenom));
        string memory ibcDenom = ICS20Lib.toIBCDenom(expectedFullDenomPath);

        vm.expectEmit(true, true, true, false); // Not checking data because we don't know the address yet
        ICS20Lib.PacketDataJSON memory packetData;
        address erc20Address;
        emit IICS20Transfer.ICS20ReceiveTransfer(packetData, erc20Address); // we check these values later
        vm.expectEmit();
        emit IICS26Router.WriteAcknowledgement(receivePacket, ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON);
        vm.expectEmit();
        emit IICS26Router.RecvPacket(receivePacket);

        vm.recordLogs();
        ics26Router.recvPacket(
            IICS26RouterMsgs.MsgRecvPacket({
                packet: receivePacket,
                proofCommitment: bytes("doesntmatter"), // dummy client will accept
                proofHeight: IICS02ClientMsgs.Height({ revisionNumber: 1, revisionHeight: 42 }) // will accept
             })
        );

        // Check that the ack is written
        bytes32 ackPath = ICS24Host.packetAcknowledgementCommitmentKeyCalldata(
            receivePacket.destPort, receivePacket.destChannel, receivePacket.sequence
        );
        bytes32 storedAck = ics26Router.getCommitment(ackPath);
        assertEq(storedAck, ICS24Host.packetAcknowledgementCommitmentBytes32(ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON));

        // find and extract data from the ICS20ReceiveTransfer event
        Vm.Log memory receiveTransferLog = vm.getRecordedLogs()[3];
        assertEq(receiveTransferLog.topics[0], IICS20Transfer.ICS20ReceiveTransfer.selector);

        (packetData, erc20Address) = abi.decode(receiveTransferLog.data, (ICS20Lib.PacketDataJSON, address));
        assertEq(packetData.denom, foreignDenom);
        assertNotEq(erc20Address, address(0));
        assertEq(packetData.sender, senderAddrStr);
        assertEq(packetData.receiver, receiverAddrStr);
        assertEq(packetData.amount, defaultAmount);
        assertEq(packetData.memo, "memo");

        IBCERC20 ibcERC20 = IBCERC20(erc20Address);
        assertEq(ibcERC20.fullDenomPath(), expectedFullDenomPath);
        assertEq(ibcERC20.name(), ICS20Lib.toIBCDenom(expectedFullDenomPath));
        assertEq(ibcERC20.symbol(), foreignDenom);
        assertEq(ibcERC20.totalSupply(), defaultAmount);
        assertEq(ibcERC20.balanceOf(receiverAddr), defaultAmount);

        // Send out again
        address backSender = receiverAddr;
        string memory backSenderStr = receiverAddrStr;
        string memory backReceiverStr = senderAddrStr;

        vm.prank(backSender);
        ibcERC20.approve(address(ics20Transfer), defaultAmount);

        IICS20TransferMsgs.SendTransferMsg memory msgSendTransfer = IICS20TransferMsgs.SendTransferMsg({
            denom: ibcDenom,
            amount: defaultAmount,
            receiver: backReceiverStr,
            sourceChannel: clientIdentifier,
            destPort: "transfer",
            timeoutTimestamp: uint64(block.timestamp + 1000),
            memo: "backmemo"
        });

        IICS26RouterMsgs.Packet memory expectedPacketSent = IICS26RouterMsgs.Packet({
            sequence: 1,
            timeoutTimestamp: msgSendTransfer.timeoutTimestamp,
            sourcePort: "transfer",
            sourceChannel: clientIdentifier,
            destPort: "transfer",
            destChannel: counterpartyClient,
            version: ICS20Lib.ICS20_VERSION,
            data: ICS20Lib.marshalJSON(expectedFullDenomPath, defaultAmount, backSenderStr, backReceiverStr, "backmemo")
        });

        vm.expectEmit();
        emit IICS20Transfer.ICS20Transfer(
            ICS20Lib.PacketDataJSON({
                denom: expectedFullDenomPath,
                sender: backSenderStr,
                receiver: backReceiverStr,
                amount: defaultAmount,
                memo: "backmemo"
            }),
            erc20Address
        );
        vm.expectEmit();
        emit IICS26Router.SendPacket(expectedPacketSent);
        vm.prank(backSender);
        uint32 sequence = ics20Transfer.sendTransfer(msgSendTransfer);
        assertEq(sequence, expectedPacketSent.sequence);

        bytes32 path = ICS24Host.packetCommitmentKeyCalldata(
            expectedPacketSent.sourcePort, expectedPacketSent.sourceChannel, expectedPacketSent.sequence
        );
        bytes32 storedCommitment = ics26Router.getCommitment(path);
        assertEq(storedCommitment, ICS24Host.packetCommitmentBytes32(expectedPacketSent));
    }

    function test_success_receiveICS20PacketWithForeignIBCDenom() public {
        string memory foreignDenom = "transfer/channel-42/uatom";

        string memory senderAddrStr = "cosmos1mhmwgrfrcrdex5gnr0vcqt90wknunsxej63feh";
        address receiverAddr = makeAddr("receiver_of_foreign_denom");
        string memory receiverAddrStr = Strings.toHexString(receiverAddr);

        // For the packet back we pretend this is ibc-go and that the timeout is in nanoseconds
        IICS26RouterMsgs.Packet memory receivePacket = IICS26RouterMsgs.Packet({
            sequence: 1,
            timeoutTimestamp: uint64(block.timestamp + 1000),
            sourcePort: "transfer",
            sourceChannel: counterpartyClient,
            destPort: "transfer",
            destChannel: clientIdentifier,
            version: ICS20Lib.ICS20_VERSION,
            data: ICS20Lib.marshalJSON(foreignDenom, defaultAmount, senderAddrStr, receiverAddrStr, "memo")
        });

        string memory expectedFullDenomPath =
            string(abi.encodePacked(receivePacket.destPort, "/", receivePacket.destChannel, "/", foreignDenom));
        string memory ibcDenom = ICS20Lib.toIBCDenom(expectedFullDenomPath);

        vm.expectEmit(true, true, true, false); // Not checking data because we don't know the address yet
        ICS20Lib.PacketDataJSON memory packetData;
        address erc20Address;
        emit IICS20Transfer.ICS20ReceiveTransfer(packetData, erc20Address); // we check these values later
        vm.expectEmit();
        emit IICS26Router.WriteAcknowledgement(receivePacket, ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON);
        vm.expectEmit();
        emit IICS26Router.RecvPacket(receivePacket);

        vm.recordLogs();
        ics26Router.recvPacket(
            IICS26RouterMsgs.MsgRecvPacket({
                packet: receivePacket,
                proofCommitment: bytes("doesntmatter"), // dummy client will accept
                proofHeight: IICS02ClientMsgs.Height({ revisionNumber: 1, revisionHeight: 42 }) // will accept
             })
        );

        // Check that the ack is written
        bytes32 ackPath = ICS24Host.packetAcknowledgementCommitmentKeyCalldata(
            receivePacket.destPort, receivePacket.destChannel, receivePacket.sequence
        );
        bytes32 storedAck = ics26Router.getCommitment(ackPath);
        assertEq(storedAck, ICS24Host.packetAcknowledgementCommitmentBytes32(ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON));

        // find and extract data from the ICS20ReceiveTransfer event
        Vm.Log memory receiveTransferLog = vm.getRecordedLogs()[3];
        assertEq(receiveTransferLog.topics[0], IICS20Transfer.ICS20ReceiveTransfer.selector);

        (packetData, erc20Address) = abi.decode(receiveTransferLog.data, (ICS20Lib.PacketDataJSON, address));
        assertEq(packetData.denom, foreignDenom);
        assertNotEq(erc20Address, address(0));
        assertEq(packetData.sender, senderAddrStr);
        assertEq(packetData.receiver, receiverAddrStr);
        assertEq(packetData.amount, defaultAmount);
        assertEq(packetData.memo, "memo");

        IBCERC20 ibcERC20 = IBCERC20(erc20Address);
        assertEq(ibcERC20.fullDenomPath(), expectedFullDenomPath);
        assertEq(ibcERC20.name(), ICS20Lib.toIBCDenom(expectedFullDenomPath));
        assertEq(ibcERC20.symbol(), foreignDenom);
        assertEq(ibcERC20.totalSupply(), defaultAmount);
        assertEq(ibcERC20.balanceOf(receiverAddr), defaultAmount);

        // Send out again
        address backSender = receiverAddr;
        string memory backSenderStr = receiverAddrStr;
        string memory backReceiverStr = senderAddrStr;

        vm.prank(backSender);
        ibcERC20.approve(address(ics20Transfer), defaultAmount);

        IICS20TransferMsgs.SendTransferMsg memory msgSendTransfer = IICS20TransferMsgs.SendTransferMsg({
            denom: ibcDenom,
            amount: defaultAmount,
            receiver: backReceiverStr,
            sourceChannel: clientIdentifier,
            destPort: "transfer",
            timeoutTimestamp: uint64(block.timestamp + 1000),
            memo: "backmemo"
        });

        IICS26RouterMsgs.Packet memory expectedPacketSent = IICS26RouterMsgs.Packet({
            sequence: 1,
            timeoutTimestamp: msgSendTransfer.timeoutTimestamp,
            sourcePort: "transfer",
            sourceChannel: clientIdentifier,
            destPort: "transfer",
            destChannel: counterpartyClient,
            version: ICS20Lib.ICS20_VERSION,
            data: ICS20Lib.marshalJSON(expectedFullDenomPath, defaultAmount, backSenderStr, backReceiverStr, "backmemo")
        });

        vm.expectEmit();
        emit IICS20Transfer.ICS20Transfer(
            ICS20Lib.PacketDataJSON({
                denom: expectedFullDenomPath,
                sender: backSenderStr,
                receiver: backReceiverStr,
                amount: defaultAmount,
                memo: "backmemo"
            }),
            erc20Address
        );
        vm.expectEmit();
        emit IICS26Router.SendPacket(expectedPacketSent);
        vm.prank(backSender);
        uint32 sequence = ics20Transfer.sendTransfer(msgSendTransfer);
        assertEq(sequence, expectedPacketSent.sequence);

        bytes32 path = ICS24Host.packetCommitmentKeyCalldata(
            expectedPacketSent.sourcePort, expectedPacketSent.sourceChannel, expectedPacketSent.sequence
        );
        bytes32 storedCommitment = ics26Router.getCommitment(path);
        assertEq(storedCommitment, ICS24Host.packetCommitmentBytes32(expectedPacketSent));
    }

    function test_failure_receiveICS20PacketHasTimedOut() public {
        IICS26RouterMsgs.Packet memory packet = _sendICS20Transfer();

        IICS26RouterMsgs.MsgAckPacket memory ackMsg = IICS26RouterMsgs.MsgAckPacket({
            packet: packet,
            acknowledgement: ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON,
            proofAcked: bytes("doesntmatter"), // dummy client will accept
            proofHeight: IICS02ClientMsgs.Height({ revisionNumber: 1, revisionHeight: 42 }) // dummy client will accept
         });
        vm.expectEmit();
        emit IICS20Transfer.ICS20Acknowledgement(
            expectedDefaultSendPacketData, ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON
        );
        ics26Router.ackPacket(ackMsg);

        // commitment should be deleted
        bytes32 path = ICS24Host.packetCommitmentKeyCalldata(
            msgSendPacket.sourcePort, msgSendPacket.sourceChannel, packet.sequence
        );
        bytes32 storedCommitment = ics26Router.getCommitment(path);
        assertEq(storedCommitment, 0);

        uint256 senderBalanceAfterSend = erc20.balanceOf(sender);
        uint256 contractBalanceAfterSend = erc20.balanceOf(address(ics20Transfer));
        assertEq(senderBalanceAfterSend, 0);
        assertEq(contractBalanceAfterSend, defaultAmount);

        // Send back
        string memory backSender = "cosmos1mhmwgrfrcrdex5gnr0vcqt90wknunsxej63feh";
        string memory backReceiverStr = senderStr;
        string memory ibcDenom = string(abi.encodePacked("transfer/", counterpartyClient, "/", erc20AddressStr));
        data = ICS20Lib.marshalJSON(ibcDenom, defaultAmount, backSender, backReceiverStr, "backmemo");

        uint64 timeoutTimestamp = uint64(block.timestamp - 1);
        packet = IICS26RouterMsgs.Packet({
            sequence: 1,
            timeoutTimestamp: timeoutTimestamp,
            sourcePort: "transfer",
            sourceChannel: counterpartyClient,
            destPort: "transfer",
            destChannel: clientIdentifier,
            version: ICS20Lib.ICS20_VERSION,
            data: data
        });

        vm.expectRevert(
            abi.encodeWithSelector(
                IICS26RouterErrors.IBCInvalidTimeoutTimestamp.selector, packet.timeoutTimestamp, block.timestamp
            )
        );
        ics26Router.recvPacket(
            IICS26RouterMsgs.MsgRecvPacket({
                packet: packet,
                proofCommitment: bytes("doesntmatter"), // dummy client will accept
                proofHeight: IICS02ClientMsgs.Height({ revisionNumber: 1, revisionHeight: 42 }) // will accept
             })
        );
    }

    function _sendICS20Transfer() internal returns (IICS26RouterMsgs.Packet memory) {
        erc20.mint(sender, defaultAmount);
        vm.startPrank(sender);
        erc20.approve(address(ics20Transfer), defaultAmount);

        uint256 senderBalanceBefore = erc20.balanceOf(sender);
        uint256 contractBalanceBefore = erc20.balanceOf(address(ics20Transfer));
        assertEq(senderBalanceBefore, defaultAmount);
        assertEq(contractBalanceBefore, 0);

        IICS20TransferMsgs.SendTransferMsg memory msgSendTransfer = IICS20TransferMsgs.SendTransferMsg({
            denom: erc20AddressStr,
            amount: defaultAmount,
            receiver: receiverStr,
            sourceChannel: clientIdentifier,
            destPort: "transfer",
            timeoutTimestamp: uint64(block.timestamp + 1000),
            memo: "memo"
        });

        vm.startPrank(sender);
        vm.expectEmit();
        emit IICS20Transfer.ICS20Transfer(expectedDefaultSendPacketData, address(erc20));
        uint32 sequence = ics20Transfer.sendTransfer(msgSendTransfer);
        assertEq(sequence, 1);

        bytes32 path =
            ICS24Host.packetCommitmentKeyCalldata(msgSendPacket.sourcePort, msgSendPacket.sourceChannel, sequence);
        bytes32 storedCommitment = ics26Router.getCommitment(path);
        IICS26RouterMsgs.Packet memory packet = _getPacket(msgSendPacket, sequence);
        assertEq(storedCommitment, ICS24Host.packetCommitmentBytes32(packet));

        return packet;
    }

    function _getPacket(
        IICS26RouterMsgs.MsgSendPacket memory _msgSendPacket,
        uint32 sequence
    )
        internal
        view
        returns (IICS26RouterMsgs.Packet memory)
    {
        return IICS26RouterMsgs.Packet({
            sequence: sequence,
            timeoutTimestamp: _msgSendPacket.timeoutTimestamp,
            sourcePort: _msgSendPacket.sourcePort,
            sourceChannel: _msgSendPacket.sourceChannel,
            destPort: _msgSendPacket.destPort,
            destChannel: counterpartyClient, // If we test with something else, we need to add this to the args
            version: _msgSendPacket.version,
            data: _msgSendPacket.data
        });
    }
}
