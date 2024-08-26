// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.25 <0.9.0;

// solhint-disable custom-errors,max-line-length,max-states-count

import { Test } from "forge-std/Test.sol";
import { IICS02Client } from "../src/interfaces/IICS02Client.sol";
import { IICS02ClientMsgs } from "../src/msgs/IICS02ClientMsgs.sol";
import { ICS20Transfer } from "../src/ICS20Transfer.sol";
import { IICS20Transfer } from "../src/interfaces/IICS20Transfer.sol";
import { IICS20Errors } from "../src/errors/IICS20Errors.sol";
import { IICS20TransferMsgs } from "../src/msgs/IICS20TransferMsgs.sol";
import { TestERC20 } from "./mocks/TestERC20.sol";
import { IBCERC20 } from "../src/utils/IBCERC20.sol";
import { ICS02Client } from "../src/ICS02Client.sol";
import { IICS26Router } from "../src/ICS26Router.sol";
import { IICS26RouterErrors } from "../src/errors/IICS26RouterErrors.sol";
import { ICS26Router } from "../src/ICS26Router.sol";
import { IICS26RouterMsgs } from "../src/msgs/IICS26RouterMsgs.sol";
import { DummyLightClient } from "./mocks/DummyLightClient.sol";
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
    bytes[] public merklePrefix = [bytes("ibc"), bytes("")];

    address public sender;
    string public senderStr;
    address public receiver;
    string public receiverStr = "someReceiver";

    /// @dev the default send amount for sendTransfer
    uint256 public transferAmount = 1_000_000_000_000_000_000;

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
            "07-tendermint", IICS02ClientMsgs.CounterpartyInfo(counterpartyClient, merklePrefix), address(lightClient)
        );
        ics20AddressStr = Strings.toHexString(address(ics20Transfer));

        vm.expectEmit();
        emit IICS26Router.IBCAppAdded("transfer", address(ics20Transfer));
        ics26Router.addIBCApp("transfer", address(ics20Transfer));

        assertEq(address(ics20Transfer), address(ics26Router.getIBCApp("transfer")));

        sender = makeAddr("sender");
        senderStr = Strings.toHexString(sender);
        data = ICS20Lib.marshalJSON(erc20AddressStr, transferAmount, senderStr, receiverStr, "memo");
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
            amount: transferAmount,
            memo: "memo"
        });
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
        assertEq(contractBalanceAfter, transferAmount);
    }

    function test_failure_sendICS20PacketDirectlyFromRouter() public {
        // We don't allow sending packets directly through the router, only through ICS20Transfer sendTransfer
        erc20.mint(sender, transferAmount);
        vm.startPrank(sender);
        erc20.approve(address(ics20Transfer), transferAmount);

        uint256 senderBalanceBefore = erc20.balanceOf(sender);
        uint256 contractBalanceBefore = erc20.balanceOf(address(ics20Transfer));
        assertEq(senderBalanceBefore, transferAmount);
        assertEq(contractBalanceBefore, 0);

        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20UnauthorizedPacketSender.selector, sender));
        ics26Router.sendPacket(msgSendPacket);
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
        assertEq(senderBalanceAfterAck, transferAmount);
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
        assertEq(senderBalanceAfterTimeout, transferAmount);
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
        assertEq(contractBalanceAfterSend, transferAmount);

        // Return the tokens (receive)
        receiverStr = senderStr;
        receiver = sender;
        senderStr = "cosmos1mhmwgrfrcrdex5gnr0vcqt90wknunsxej63feh";
        string memory receivedDenom = string(abi.encodePacked("transfer/", counterpartyClient, "/", erc20AddressStr));

        // For the packet back we pretend this is ibc-go and that the timeout is in nanoseconds
        packet = IICS26RouterMsgs.Packet({
            sequence: 1,
            timeoutTimestamp: packet.timeoutTimestamp + 1000,
            sourcePort: "transfer",
            sourceChannel: counterpartyClient,
            destPort: "transfer",
            destChannel: clientIdentifier,
            version: ICS20Lib.ICS20_VERSION,
            data: ICS20Lib.marshalJSON(receivedDenom, transferAmount, senderStr, receiverStr, "backmemo")
        });
        vm.expectEmit();
        emit IICS20Transfer.ICS20ReceiveTransfer(
            ICS20Lib.PacketDataJSON({
                denom: receivedDenom,
                sender: senderStr,
                receiver: receiverStr,
                amount: transferAmount,
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
        assertEq(erc20.balanceOf(receiver), transferAmount);
        assertEq(erc20.balanceOf(address(ics20Transfer)), 0);

        // Check that the ack is written
        bytes32 storedAck = ics26Router.getCommitment(
            ICS24Host.packetAcknowledgementCommitmentKeyCalldata(packet.destPort, packet.destChannel, packet.sequence)
        );
        assertEq(storedAck, ICS24Host.packetAcknowledgementCommitmentBytes32(ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON));
    }

    function test_success_receiveICS20PacketWithForeignBaseDenom() public {
        string memory foreignDenom = "uatom";

        senderStr = "cosmos1mhmwgrfrcrdex5gnr0vcqt90wknunsxej63feh";
        receiver = makeAddr("receiver_of_foreign_denom");
        receiverStr = Strings.toHexString(receiver);

        // For the packet back we pretend this is ibc-go and that the timeout is in nanoseconds
        IICS26RouterMsgs.Packet memory receivePacket = IICS26RouterMsgs.Packet({
            sequence: 1,
            timeoutTimestamp: uint64(block.timestamp + 1000),
            sourcePort: "transfer",
            sourceChannel: counterpartyClient,
            destPort: "transfer",
            destChannel: clientIdentifier,
            version: ICS20Lib.ICS20_VERSION,
            data: ICS20Lib.marshalJSON(foreignDenom, transferAmount, senderStr, receiverStr, "memo")
        });

        string memory expectedFullDenomPath =
            string(abi.encodePacked(receivePacket.destPort, "/", receivePacket.destChannel, "/", foreignDenom));

        ICS20Lib.PacketDataJSON memory packetData;
        address erc20Address;
        vm.expectEmit(true, true, true, false); // Not checking data because we don't know the address yet
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
        bytes32 storedAck = ics26Router.getCommitment(
            ICS24Host.packetAcknowledgementCommitmentKeyCalldata(
                receivePacket.destPort, receivePacket.destChannel, receivePacket.sequence
            )
        );
        assertEq(storedAck, ICS24Host.packetAcknowledgementCommitmentBytes32(ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON));

        // find and extract data from the ICS20ReceiveTransfer event
        Vm.Log memory receiveTransferLog = vm.getRecordedLogs()[3];
        assertEq(receiveTransferLog.topics[0], IICS20Transfer.ICS20ReceiveTransfer.selector);

        (packetData, erc20Address) = abi.decode(receiveTransferLog.data, (ICS20Lib.PacketDataJSON, address));
        assertEq(packetData.denom, foreignDenom);
        assertNotEq(erc20Address, address(0));
        assertEq(packetData.sender, senderStr);
        assertEq(packetData.receiver, receiverStr);
        assertEq(packetData.amount, transferAmount);
        assertEq(packetData.memo, "memo");

        IBCERC20 ibcERC20 = IBCERC20(erc20Address);
        assertEq(ibcERC20.fullDenomPath(), expectedFullDenomPath);
        assertEq(ibcERC20.name(), ICS20Lib.toIBCDenom(expectedFullDenomPath));
        assertEq(ibcERC20.symbol(), foreignDenom);
        assertEq(ibcERC20.totalSupply(), transferAmount);
        assertEq(ibcERC20.balanceOf(receiver), transferAmount);

        // Send out again
        string memory backDenom = Strings.toHexString(erc20Address); // sendTransfer use the contract as the denom
        sender = receiver;
        {
            string memory tmpSenderStr = senderStr;
            senderStr = receiverStr;
            receiverStr = tmpSenderStr;
        }

        vm.prank(sender);
        ibcERC20.approve(address(ics20Transfer), transferAmount);

        IICS20TransferMsgs.SendTransferMsg memory msgSendTransfer = IICS20TransferMsgs.SendTransferMsg({
            denom: backDenom,
            amount: transferAmount,
            receiver: receiverStr,
            sourceChannel: clientIdentifier,
            destPort: "transfer",
            timeoutTimestamp: uint64(block.timestamp + 1000),
            memo: "backmemo"
        });

        vm.expectEmit();
        emit IICS20Transfer.ICS20Transfer(
            ICS20Lib.PacketDataJSON({
                denom: expectedFullDenomPath,
                sender: senderStr,
                receiver: receiverStr,
                amount: transferAmount,
                memo: "backmemo"
            }),
            erc20Address
        );
        IICS26RouterMsgs.Packet memory expectedPacketSent = IICS26RouterMsgs.Packet({
            sequence: 1,
            timeoutTimestamp: msgSendTransfer.timeoutTimestamp,
            sourcePort: "transfer",
            sourceChannel: clientIdentifier,
            destPort: "transfer",
            destChannel: counterpartyClient,
            version: ICS20Lib.ICS20_VERSION,
            data: ICS20Lib.marshalJSON(expectedFullDenomPath, transferAmount, senderStr, receiverStr, "backmemo")
        });
        vm.expectEmit();
        emit IICS26Router.SendPacket(expectedPacketSent);
        vm.prank(sender);
        uint32 sequence = ics20Transfer.sendTransfer(msgSendTransfer);
        assertEq(sequence, expectedPacketSent.sequence);

        assertEq(ibcERC20.totalSupply(), 0);
        assertEq(ibcERC20.balanceOf(sender), 0);

        bytes32 path = ICS24Host.packetCommitmentKeyCalldata(
            expectedPacketSent.sourcePort, expectedPacketSent.sourceChannel, expectedPacketSent.sequence
        );
        bytes32 storedCommitment = ics26Router.getCommitment(path);
        assertEq(storedCommitment, ICS24Host.packetCommitmentBytes32(expectedPacketSent));
    }

    function test_success_receiveICS20PacketWithForeignIBCDenom() public {
        string memory foreignDenom = "transfer/channel-42/uatom";

        senderStr = "cosmos1mhmwgrfrcrdex5gnr0vcqt90wknunsxej63feh";
        receiver = makeAddr("receiver_of_foreign_denom");
        receiverStr = Strings.toHexString(receiver);

        // For the packet back we pretend this is ibc-go and that the timeout is in nanoseconds
        IICS26RouterMsgs.Packet memory receivePacket = IICS26RouterMsgs.Packet({
            sequence: 1,
            timeoutTimestamp: uint64(block.timestamp + 1000),
            sourcePort: "transfer",
            sourceChannel: counterpartyClient,
            destPort: "transfer",
            destChannel: clientIdentifier,
            version: ICS20Lib.ICS20_VERSION,
            data: ICS20Lib.marshalJSON(foreignDenom, transferAmount, senderStr, receiverStr, "memo")
        });

        string memory expectedFullDenomPath =
            string(abi.encodePacked(receivePacket.destPort, "/", receivePacket.destChannel, "/", foreignDenom));

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
        bytes32 storedAck = ics26Router.getCommitment(
            ICS24Host.packetAcknowledgementCommitmentKeyCalldata(
                receivePacket.destPort, receivePacket.destChannel, receivePacket.sequence
            )
        );
        assertEq(storedAck, ICS24Host.packetAcknowledgementCommitmentBytes32(ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON));

        // find and extract data from the ICS20ReceiveTransfer event
        Vm.Log memory receiveTransferLog = vm.getRecordedLogs()[3];
        assertEq(receiveTransferLog.topics[0], IICS20Transfer.ICS20ReceiveTransfer.selector);

        (packetData, erc20Address) = abi.decode(receiveTransferLog.data, (ICS20Lib.PacketDataJSON, address));
        assertEq(packetData.denom, foreignDenom);
        assertNotEq(erc20Address, address(0));
        assertEq(packetData.sender, senderStr);
        assertEq(packetData.receiver, receiverStr);
        assertEq(packetData.amount, transferAmount);
        assertEq(packetData.memo, "memo");

        IBCERC20 ibcERC20 = IBCERC20(erc20Address);
        assertEq(ibcERC20.fullDenomPath(), expectedFullDenomPath);
        assertEq(ibcERC20.name(), ICS20Lib.toIBCDenom(expectedFullDenomPath));
        assertEq(ibcERC20.symbol(), foreignDenom);
        assertEq(ibcERC20.totalSupply(), transferAmount);
        assertEq(ibcERC20.balanceOf(receiver), transferAmount);

        // Send out again
        string memory backDenom = Strings.toHexString(erc20Address); // sendTransfer use the contract as the denom
        sender = receiver;
        {
            string memory tmpSenderStr = senderStr;
            senderStr = receiverStr;
            receiverStr = tmpSenderStr;
        }

        vm.prank(sender);
        ibcERC20.approve(address(ics20Transfer), transferAmount);

        IICS20TransferMsgs.SendTransferMsg memory msgSendTransfer = IICS20TransferMsgs.SendTransferMsg({
            denom: backDenom,
            amount: transferAmount,
            receiver: receiverStr,
            sourceChannel: clientIdentifier,
            destPort: "transfer",
            timeoutTimestamp: uint64(block.timestamp + 1000),
            memo: "backmemo"
        });

        vm.expectEmit();
        emit IICS20Transfer.ICS20Transfer(
            ICS20Lib.PacketDataJSON({
                denom: expectedFullDenomPath,
                sender: senderStr,
                receiver: receiverStr,
                amount: transferAmount,
                memo: "backmemo"
            }),
            erc20Address
        );

        vm.expectEmit();
        IICS26RouterMsgs.Packet memory expectedPacketSent = IICS26RouterMsgs.Packet({
            sequence: 1,
            timeoutTimestamp: msgSendTransfer.timeoutTimestamp,
            sourcePort: "transfer",
            sourceChannel: clientIdentifier,
            destPort: "transfer",
            destChannel: counterpartyClient,
            version: ICS20Lib.ICS20_VERSION,
            data: ICS20Lib.marshalJSON(expectedFullDenomPath, transferAmount, senderStr, receiverStr, "backmemo")
        });
        emit IICS26Router.SendPacket(expectedPacketSent);

        vm.prank(sender);
        uint32 sequence = ics20Transfer.sendTransfer(msgSendTransfer);

        assertEq(sequence, expectedPacketSent.sequence);

        assertEq(ibcERC20.totalSupply(), 0);
        assertEq(ibcERC20.balanceOf(receiver), 0);

        bytes32 path = ICS24Host.packetCommitmentKeyCalldata(
            expectedPacketSent.sourcePort, expectedPacketSent.sourceChannel, expectedPacketSent.sequence
        );
        bytes32 storedCommitment = ics26Router.getCommitment(path);
        assertEq(storedCommitment, ICS24Host.packetCommitmentBytes32(expectedPacketSent));
    }

    function test_success_receiveICS20PacketWithLargeAmountAndForeignIBCDenom() public {
        string memory foreignDenom = "transfer/channel-42/uatom";

        senderStr = "cosmos1mhmwgrfrcrdex5gnr0vcqt90wknunsxej63feh";
        receiver = makeAddr("receiver_of_foreign_denom");
        receiverStr = Strings.toHexString(receiver);

        uint256 largeAmount = 1_000_000_000_000_000_001;

        // For the packet back we pretend this is ibc-go and that the timeout is in nanoseconds
        IICS26RouterMsgs.Packet memory receivePacket = IICS26RouterMsgs.Packet({
            sequence: 1,
            timeoutTimestamp: uint64(block.timestamp + 1000),
            sourcePort: "transfer",
            sourceChannel: counterpartyClient,
            destPort: "transfer",
            destChannel: clientIdentifier,
            version: ICS20Lib.ICS20_VERSION,
            data: ICS20Lib.marshalJSON(foreignDenom, largeAmount, senderStr, receiverStr, "")
        });

        string memory expectedFullDenomPath =
            string(abi.encodePacked(receivePacket.destPort, "/", receivePacket.destChannel, "/", foreignDenom));

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
        bytes32 storedAck = ics26Router.getCommitment(
            ICS24Host.packetAcknowledgementCommitmentKeyCalldata(
                receivePacket.destPort, receivePacket.destChannel, receivePacket.sequence
            )
        );
        assertEq(storedAck, ICS24Host.packetAcknowledgementCommitmentBytes32(ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON));

        // find and extract data from the ICS20ReceiveTransfer event
        Vm.Log memory receiveTransferLog = vm.getRecordedLogs()[3];
        assertEq(receiveTransferLog.topics[0], IICS20Transfer.ICS20ReceiveTransfer.selector);

        (packetData, erc20Address) = abi.decode(receiveTransferLog.data, (ICS20Lib.PacketDataJSON, address));
        assertEq(packetData.denom, foreignDenom);
        assertNotEq(erc20Address, address(0));
        assertEq(packetData.sender, senderStr);
        assertEq(packetData.receiver, receiverStr);
        assertEq(packetData.amount, largeAmount);
        assertEq(packetData.memo, "");

        IBCERC20 ibcERC20 = IBCERC20(erc20Address);
        assertEq(ibcERC20.fullDenomPath(), expectedFullDenomPath);
        assertEq(ibcERC20.name(), ICS20Lib.toIBCDenom(expectedFullDenomPath));
        assertEq(ibcERC20.symbol(), foreignDenom);
        assertEq(ibcERC20.totalSupply(), largeAmount);
        assertEq(ibcERC20.balanceOf(receiver), largeAmount);

        // Send out again
        string memory backDenom = Strings.toHexString(erc20Address); // sendTransfer use the contract as the denom
        sender = receiver;
        {
            string memory tmpSenderStr = senderStr;
            senderStr = receiverStr;
            receiverStr = tmpSenderStr;
        }

        vm.prank(sender);
        ibcERC20.approve(address(ics20Transfer), largeAmount);

        IICS20TransferMsgs.SendTransferMsg memory msgSendTransfer = IICS20TransferMsgs.SendTransferMsg({
            denom: backDenom,
            amount: largeAmount,
            receiver: receiverStr,
            sourceChannel: clientIdentifier,
            destPort: "transfer",
            timeoutTimestamp: uint64(block.timestamp + 1000),
            memo: ""
        });

        vm.expectEmit();
        emit IICS20Transfer.ICS20Transfer(
            ICS20Lib.PacketDataJSON({
                denom: expectedFullDenomPath,
                sender: senderStr,
                receiver: receiverStr,
                amount: largeAmount,
                memo: ""
            }),
            erc20Address
        );

        vm.expectEmit();
        IICS26RouterMsgs.Packet memory expectedPacketSent = IICS26RouterMsgs.Packet({
            sequence: 1,
            timeoutTimestamp: msgSendTransfer.timeoutTimestamp,
            sourcePort: "transfer",
            sourceChannel: clientIdentifier,
            destPort: "transfer",
            destChannel: counterpartyClient,
            version: ICS20Lib.ICS20_VERSION,
            data: ICS20Lib.marshalJSON(expectedFullDenomPath, largeAmount, senderStr, receiverStr, "")
        });
        emit IICS26Router.SendPacket(expectedPacketSent);

        vm.prank(sender);
        uint32 sequence = ics20Transfer.sendTransfer(msgSendTransfer);

        assertEq(sequence, expectedPacketSent.sequence);

        assertEq(ibcERC20.totalSupply(), 0);
        assertEq(ibcERC20.balanceOf(receiver), 0);

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
        assertEq(contractBalanceAfterSend, transferAmount);

        // Send back
        receiverStr = senderStr;
        senderStr = "cosmos1mhmwgrfrcrdex5gnr0vcqt90wknunsxej63feh";
        string memory ibcDenom = string(abi.encodePacked("transfer/", counterpartyClient, "/", erc20AddressStr));
        data = ICS20Lib.marshalJSON(ibcDenom, transferAmount, senderStr, receiverStr, "backmemo");

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
        erc20.mint(sender, transferAmount);
        vm.startPrank(sender);
        erc20.approve(address(ics20Transfer), transferAmount);

        uint256 senderBalanceBefore = erc20.balanceOf(sender);
        uint256 contractBalanceBefore = erc20.balanceOf(address(ics20Transfer));
        assertEq(senderBalanceBefore, transferAmount);
        assertEq(contractBalanceBefore, 0);

        IICS20TransferMsgs.SendTransferMsg memory msgSendTransfer = IICS20TransferMsgs.SendTransferMsg({
            denom: erc20AddressStr,
            amount: transferAmount,
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
