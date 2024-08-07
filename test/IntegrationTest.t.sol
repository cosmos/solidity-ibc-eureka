// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.25 <0.9.0;

// solhint-disable custom-errors,max-line-length

import { Test } from "forge-std/Test.sol";
import { IICS02Client } from "../src/interfaces/IICS02Client.sol";
import { IICS02ClientMsgs } from "../src/msgs/IICS02ClientMsgs.sol";
import { ICS20Transfer } from "../src/ICS20Transfer.sol";
import { IICS20Transfer } from "../src/interfaces/IICS20Transfer.sol";
import { IICS20TransferMsgs } from "../src/msgs/IICS20TransferMsgs.sol";
import { TestERC20 } from "./TestERC20.sol";
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
    string public receiver = "someReceiver";
    bytes public data;
    IICS26RouterMsgs.MsgSendPacket public msgSendPacket;

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
        data = ICS20Lib.marshalJSON(erc20AddressStr, defaultAmount, senderStr, receiver, "memo");
        msgSendPacket = IICS26RouterMsgs.MsgSendPacket({
            sourcePort: "transfer",
            sourceChannel: clientIdentifier,
            destPort: "transfer",
            data: data,
            timeoutTimestamp: uint64(block.timestamp + 1000),
            version: ICS20Lib.ICS20_VERSION
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
        emit IICS20Transfer.ICS20Transfer(_getPacketData());
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
        emit IICS20Transfer.ICS20Acknowledgement(_getPacketData(), ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON, true);
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
        emit IICS20Transfer.ICS20Acknowledgement(_getPacketData(), ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON, true);
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
        emit IICS20Transfer.ICS20Acknowledgement(_getPacketData(), ICS20Lib.FAILED_ACKNOWLEDGEMENT_JSON, false);
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
        emit IICS20Transfer.ICS20Timeout(_getPacketData());
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

    function test_success_receiveICS20PacketWithKnownDenom() public {
        IICS26RouterMsgs.Packet memory packet = _sendICS20Transfer();

        IICS26RouterMsgs.MsgAckPacket memory ackMsg = IICS26RouterMsgs.MsgAckPacket({
            packet: packet,
            acknowledgement: ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON,
            proofAcked: bytes("doesntmatter"), // dummy client will accept
            proofHeight: IICS02ClientMsgs.Height({ revisionNumber: 1, revisionHeight: 42 }) // dummy client will accept
         });
        vm.expectEmit();
        emit IICS20Transfer.ICS20Acknowledgement(_getPacketData(), ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON, true);
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
        string memory ibcDenom = string(abi.encodePacked("transfer/", counterpartyClient, "/", erc20AddressStr));
        data = ICS20Lib.marshalJSON(ibcDenom, defaultAmount, backSender, backReceiverStr, "backmemo");

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
                denom: ibcDenom,
                amount: defaultAmount,
                sender: backSender,
                receiver: backReceiverStr,
                memo: "backmemo"
            })
        );
        vm.expectEmit();
        emit IICS26Router.WriteAcknowledgement(packet, ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON);
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

    function test_failure_receiveICS20PacketHasTimedOut() public {
        IICS26RouterMsgs.Packet memory packet = _sendICS20Transfer();

        IICS26RouterMsgs.MsgAckPacket memory ackMsg = IICS26RouterMsgs.MsgAckPacket({
            packet: packet,
            acknowledgement: ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON,
            proofAcked: bytes("doesntmatter"), // dummy client will accept
            proofHeight: IICS02ClientMsgs.Height({ revisionNumber: 1, revisionHeight: 42 }) // dummy client will accept
         });
        vm.expectEmit();
        emit IICS20Transfer.ICS20Acknowledgement(_getPacketData(), ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON, true);
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
            receiver: receiver,
            sourceChannel: clientIdentifier,
            destPort: "transfer",
            timeoutTimestamp: uint64(block.timestamp + 1000),
            memo: "memo"
        });

        vm.startPrank(sender);
        vm.expectEmit();
        emit IICS20Transfer.ICS20Transfer(_getPacketData());
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

    function _getPacketData() internal view returns (ICS20Lib.UnwrappedFungibleTokenPacketData memory) {
        return ICS20Lib.UnwrappedFungibleTokenPacketData({
            sender: sender,
            receiver: receiver,
            erc20ContractAddress: address(erc20),
            amount: defaultAmount,
            memo: "memo"
        });
    }
}
