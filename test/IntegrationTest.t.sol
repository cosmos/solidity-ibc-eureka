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
import { ICS26Router } from "../src/ICS26Router.sol";
import { IICS26RouterMsgs } from "../src/msgs/IICS26RouterMsgs.sol";
import { DummyLightClient } from "./DummyLightClient.sol";
import { ILightClientMsgs } from "../src/msgs/ILightClientMsgs.sol";
import { ICS20Lib } from "../src/utils/ICS20Lib.sol";
import { ICS24Host } from "../src/utils/ICS24Host.sol";

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
        lightClient = new DummyLightClient(ILightClientMsgs.UpdateResult.Update, 0);
        ics20Transfer = new ICS20Transfer(address(ics26Router));
        erc20 = new TestERC20();
        erc20AddressStr = ICS20Lib.addressToHexString(address(erc20));

        clientIdentifier = ics02Client.addClient(
            "07-tendermint", IICS02ClientMsgs.CounterpartyInfo(counterpartyClient), address(lightClient)
        );
        ics20AddressStr = ICS20Lib.addressToHexString(address(ics20Transfer));
        ics26Router.addIBCApp("", address(ics20Transfer));

        assertEq(address(ics20Transfer), address(ics26Router.getIBCApp(ics20AddressStr)));

        sender = makeAddr("sender");
        senderStr = ICS20Lib.addressToHexString(sender);
        data = ICS20Lib.marshalJSON(erc20AddressStr, defaultAmount, senderStr, receiver, "memo");
        msgSendPacket = IICS26RouterMsgs.MsgSendPacket({
            sourcePort: ics20AddressStr,
            sourceChannel: clientIdentifier,
            destPort: "transfer",
            data: data,
            timeoutTimestamp: uint32(block.timestamp) + 1000,
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

    function test_success_sendICS20PacketFromICSContract() public {
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
        lightClient.setMembershipResult(msgSendPacket.timeoutTimestamp + 1);

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
            timeoutTimestamp: uint32(block.timestamp) + 1000,
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
