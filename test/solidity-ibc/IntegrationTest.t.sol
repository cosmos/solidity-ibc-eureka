// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

// solhint-disable custom-errors,max-line-length,max-states-count

import { Test } from "forge-std/Test.sol";
import { IICS02ClientMsgs } from "../../contracts/msgs/IICS02ClientMsgs.sol";
import { ICS20Transfer } from "../../contracts/ICS20Transfer.sol";
import { IICS20TransferMsgs } from "../../contracts/msgs/IICS20TransferMsgs.sol";
import { TestERC20 } from "./mocks/TestERC20.sol";
import { IBCERC20 } from "../../contracts/utils/IBCERC20.sol";
import { IICS26Router } from "../../contracts/interfaces/IICS26Router.sol";
import { IIBCStore } from "../../contracts/interfaces/IIBCStore.sol";
import { IICS26RouterErrors } from "../../contracts/errors/IICS26RouterErrors.sol";
import { ICS26Router } from "../../contracts/ICS26Router.sol";
import { ICS02Client } from "../../contracts/ICS02Client.sol";
import { IICS26RouterMsgs } from "../../contracts/msgs/IICS26RouterMsgs.sol";
import { DummyLightClient } from "./mocks/DummyLightClient.sol";
import { ErroneousIBCStore } from "./mocks/ErroneousIBCStore.sol";
import { ILightClientMsgs } from "../../contracts/msgs/ILightClientMsgs.sol";
import { ICS20Lib } from "../../contracts/utils/ICS20Lib.sol";
import { ICS24Host } from "../../contracts/utils/ICS24Host.sol";
import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";
import { TransparentUpgradeableProxy } from "@openzeppelin-contracts/proxy/transparent/TransparentUpgradeableProxy.sol";

contract IntegrationTest is Test {
    ICS26Router public ics26Router;
    DummyLightClient public lightClient;
    string public clientIdentifier;
    ICS20Transfer public ics20Transfer;
    string public ics20AddressStr;
    TestERC20 public erc20;
    string public erc20AddressStr;
    string public counterpartyId = "42-dummy-01";
    bytes[] public merklePrefix = [bytes("ibc"), bytes("")];
    bytes[] public singleSuccessAck = [ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON];

    address public sender;
    string public senderStr;
    address public receiver;
    string public receiverStr = "someReceiver";

    /// @dev the default send amount for sendTransfer
    uint256 public transferAmount = 1_000_000_000_000_000_000;

    ICS20Lib.FungibleTokenPacketData public expectedDefaultSendPacketData;

    function setUp() public {
        // ============ Step 1: Deploy the logic contracts ==============
        lightClient = new DummyLightClient(ILightClientMsgs.UpdateResult.Update, 0, false);
        ICS02Client ics02ClientLogic = new ICS02Client();
        ICS26Router ics26RouterLogic = new ICS26Router();
        ICS20Transfer ics20TransferLogic = new ICS20Transfer();

        // ============== Step 2: Deploy Transparent Proxies ==============
        TransparentUpgradeableProxy coreProxy = new TransparentUpgradeableProxy(
            address(ics02ClientLogic),
            address(this),
            abi.encodeWithSelector(ICS02Client.initialize.selector, address(this))
        );

        TransparentUpgradeableProxy routerProxy = new TransparentUpgradeableProxy(
            address(ics26RouterLogic),
            address(this),
            abi.encodeWithSelector(ICS26Router.initialize.selector, address(this), address(coreProxy))
        );

        TransparentUpgradeableProxy transferProxy = new TransparentUpgradeableProxy(
            address(ics20TransferLogic),
            address(this),
            abi.encodeWithSelector(ICS20Transfer.initialize.selector, address(routerProxy))
        );

        // ============== Step 3: Wire up the contracts ==============
        ics26Router = ICS26Router(address(routerProxy));
        ics20Transfer = ICS20Transfer(address(transferProxy));
        erc20 = new TestERC20();
        erc20AddressStr = Strings.toHexString(address(erc20));

        clientIdentifier = ics26Router.ICS02_CLIENT().addClient(
            "07-tendermint", IICS02ClientMsgs.CounterpartyInfo(counterpartyId, merklePrefix), address(lightClient)
        );
        ics20AddressStr = Strings.toHexString(address(ics20Transfer));

        vm.expectEmit();
        emit IICS26Router.IBCAppAdded(ICS20Lib.DEFAULT_PORT_ID, address(ics20Transfer));
        ics26Router.addIBCApp(ICS20Lib.DEFAULT_PORT_ID, address(ics20Transfer));

        assertEq(address(ics20Transfer), address(ics26Router.getIBCApp(ICS20Lib.DEFAULT_PORT_ID)));

        sender = makeAddr("sender");
        senderStr = Strings.toHexString(sender);

        expectedDefaultSendPacketData = ICS20Lib.FungibleTokenPacketData({
            denom: erc20AddressStr,
            sender: senderStr,
            receiver: receiverStr,
            amount: transferAmount,
            memo: "memo"
        });
    }

    function test_success_sendICS20PacketDirectlyFromRouter() public {
        IICS26RouterMsgs.Packet memory packet = _sendICS20TransferPacket();

        IICS26RouterMsgs.MsgAckPacket memory ackMsg = IICS26RouterMsgs.MsgAckPacket({
            packet: packet,
            acknowledgement: ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON,
            proofAcked: bytes("doesntmatter"), // dummy client will accept
            proofHeight: IICS02ClientMsgs.Height({ revisionNumber: 1, revisionHeight: 42 }) // dummy client will accept
         });
        ics26Router.ackPacket(ackMsg);
        // commitment should be deleted
        bytes32 path = ICS24Host.packetCommitmentKeyCalldata(packet.sourceChannel, packet.sequence);
        bytes32 storedCommitment = ics26Router.IBC_STORE().getCommitment(path);
        assertEq(storedCommitment, 0);

        uint256 senderBalanceAfter = erc20.balanceOf(sender);
        uint256 contractBalanceAfter = erc20.balanceOf(ics20Transfer.escrow());
        assertEq(senderBalanceAfter, 0);
        assertEq(contractBalanceAfter, transferAmount);
    }

    function test_success_sendICS20PacketFromICS20Contract() public {
        erc20.mint(sender, transferAmount);
        vm.startPrank(sender);
        erc20.approve(address(ics20Transfer), transferAmount);

        uint256 senderBalanceBefore = erc20.balanceOf(sender);
        uint256 contractBalanceBefore = erc20.balanceOf(ics20Transfer.escrow());
        assertEq(senderBalanceBefore, transferAmount);
        assertEq(contractBalanceBefore, 0);

        IICS20TransferMsgs.SendTransferMsg memory transferMsg = IICS20TransferMsgs.SendTransferMsg({
            denom: erc20AddressStr,
            amount: transferAmount,
            receiver: receiverStr,
            sourceChannel: clientIdentifier,
            destPort: ICS20Lib.DEFAULT_PORT_ID,
            timeoutTimestamp: uint64(block.timestamp + 1000),
            memo: "memo"
        });

        vm.startPrank(sender);
        uint32 sequence = ics20Transfer.sendTransfer(transferMsg);
        assertEq(sequence, 1);

        ICS20Lib.FungibleTokenPacketData memory packetData = ICS20Lib.FungibleTokenPacketData({
            denom: erc20AddressStr,
            sender: senderStr,
            receiver: receiverStr,
            amount: transferAmount,
            memo: "memo"
        });

        IICS26RouterMsgs.Payload[] memory packetPayloads = new IICS26RouterMsgs.Payload[](1);
        packetPayloads[0] = IICS26RouterMsgs.Payload({
            sourcePort: "transfer",
            destPort: "transfer",
            version: ICS20Lib.ICS20_VERSION,
            encoding: ICS20Lib.ICS20_ENCODING,
            value: abi.encode(packetData)
        });
        IICS26RouterMsgs.Packet memory packet = IICS26RouterMsgs.Packet({
            sequence: sequence,
            sourceChannel: transferMsg.sourceChannel,
            destChannel: counterpartyId, // If we test with something else, we need to add this to the args
            timeoutTimestamp: transferMsg.timeoutTimestamp,
            payloads: packetPayloads
        });

        bytes32 path = ICS24Host.packetCommitmentKeyCalldata(transferMsg.sourceChannel, sequence);
        bytes32 storedCommitment = ics26Router.IBC_STORE().getCommitment(path);
        assertEq(storedCommitment, ICS24Host.packetCommitmentBytes32(packet));

        IICS26RouterMsgs.MsgAckPacket memory ackMsg = IICS26RouterMsgs.MsgAckPacket({
            packet: packet,
            acknowledgement: ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON,
            proofAcked: bytes("doesntmatter"), // dummy client will accept
            proofHeight: IICS02ClientMsgs.Height({ revisionNumber: 1, revisionHeight: 42 }) // dummy client will accept
         });

        ics26Router.ackPacket(ackMsg);

        // commitment should be deleted
        storedCommitment = ics26Router.IBC_STORE().getCommitment(path);
        assertEq(storedCommitment, 0);

        uint256 senderBalanceAfter = erc20.balanceOf(sender);
        uint256 contractBalanceAfter = erc20.balanceOf(ics20Transfer.escrow());
        assertEq(senderBalanceAfter, 0);
        assertEq(contractBalanceAfter, transferAmount);
    }

    function test_failure_sendPacketWithLargeTimeoutDuration() public {
        uint64 timeoutTimestamp = uint64(block.timestamp + 2 days);
        IICS26RouterMsgs.MsgSendPacket memory msgSendPacket = ICS20Lib.newMsgSendPacketV1(
            sender,
            IICS20TransferMsgs.SendTransferMsg({
                denom: erc20AddressStr,
                amount: transferAmount,
                receiver: receiverStr,
                sourceChannel: clientIdentifier,
                destPort: ICS20Lib.DEFAULT_PORT_ID,
                timeoutTimestamp: timeoutTimestamp,
                memo: "memo"
            })
        );

        vm.expectRevert(abi.encodeWithSelector(IICS26RouterErrors.IBCInvalidTimeoutDuration.selector, 1 days, 2 days));
        ics26Router.sendPacket(msgSendPacket);
    }

    function test_success_failedCounterpartyAckForICS20Packet() public {
        IICS26RouterMsgs.Packet memory packet = _sendICS20TransferPacket();

        IICS26RouterMsgs.MsgAckPacket memory ackMsg = IICS26RouterMsgs.MsgAckPacket({
            packet: packet,
            acknowledgement: ICS20Lib.FAILED_ACKNOWLEDGEMENT_JSON,
            proofAcked: bytes("doesntmatter"), // dummy client will accept
            proofHeight: IICS02ClientMsgs.Height({ revisionNumber: 1, revisionHeight: 42 }) // dummy client will accept
         });

        ics26Router.ackPacket(ackMsg);
        // commitment should be deleted
        bytes32 path = ICS24Host.packetCommitmentKeyCalldata(packet.sourceChannel, packet.sequence);
        bytes32 storedCommitment = ics26Router.IBC_STORE().getCommitment(path);
        assertEq(storedCommitment, 0);

        // transfer should be reverted
        uint256 senderBalanceAfterAck = erc20.balanceOf(sender);
        uint256 contractBalanceAfterAck = erc20.balanceOf(ics20Transfer.escrow());
        assertEq(senderBalanceAfterAck, transferAmount);
        assertEq(contractBalanceAfterAck, 0);
    }

    function test_success_ackNoop() public {
        IICS26RouterMsgs.Packet memory packet = _sendICS20TransferPacket();

        IICS26RouterMsgs.MsgAckPacket memory ackMsg = IICS26RouterMsgs.MsgAckPacket({
            packet: packet,
            acknowledgement: ICS20Lib.FAILED_ACKNOWLEDGEMENT_JSON,
            proofAcked: bytes("doesntmatter"), // dummy client will accept
            proofHeight: IICS02ClientMsgs.Height({ revisionNumber: 1, revisionHeight: 42 }) // dummy client will accept
         });
        ics26Router.ackPacket(ackMsg);
        // commitment should be deleted
        bytes32 path = ICS24Host.packetCommitmentKeyCalldata(packet.sourceChannel, packet.sequence);
        bytes32 storedCommitment = ics26Router.IBC_STORE().getCommitment(path);
        assertEq(storedCommitment, 0);

        // call ack again, should be noop
        vm.expectEmit();
        emit IICS26Router.Noop();
        ics26Router.ackPacket(ackMsg);
    }

    // This test case tests the scenario where IBCStore.deletePacketCommitment fails with custom error
    function test_failure_ackNoop() public {
        IICS26RouterMsgs.Packet memory packet = _sendICS20TransferPacket();

        IICS26RouterMsgs.MsgAckPacket memory ackMsg = IICS26RouterMsgs.MsgAckPacket({
            packet: packet,
            acknowledgement: ICS20Lib.FAILED_ACKNOWLEDGEMENT_JSON,
            proofAcked: bytes("doesntmatter"), // dummy client will accept
            proofHeight: IICS02ClientMsgs.Height({ revisionNumber: 1, revisionHeight: 42 }) // dummy client will accept
         });
        ics26Router.ackPacket(ackMsg);
        // commitment should be deleted
        bytes32 path = ICS24Host.packetCommitmentKeyCalldata(packet.sourceChannel, packet.sequence);
        bytes32 storedCommitment = ics26Router.IBC_STORE().getCommitment(path);
        assertEq(storedCommitment, 0);

        // override IBCStore to ErroneousIBCStore
        vm.mockFunction(
            address(ics26Router.IBC_STORE()),
            address(new ErroneousIBCStore()),
            abi.encodeWithSelector(IIBCStore.deletePacketCommitment.selector)
        );
        // call ack again, should throw CallFailure
        vm.expectRevert(abi.encodeWithSelector(ErroneousIBCStore.CallFailure.selector, "deletePacketCommitment"));
        ics26Router.ackPacket(ackMsg);
    }

    function test_success_timeoutICS20Packet() public {
        IICS26RouterMsgs.Packet memory packet = _sendICS20TransferPacket();

        // make light client return timestamp that is after our timeout
        lightClient.setMembershipResult(packet.timeoutTimestamp + 1, false);

        IICS26RouterMsgs.MsgTimeoutPacket memory timeoutMsg = IICS26RouterMsgs.MsgTimeoutPacket({
            packet: packet,
            proofTimeout: bytes("doesntmatter"), // dummy client will accept
            proofHeight: IICS02ClientMsgs.Height({ revisionNumber: 1, revisionHeight: 42 }) // dummy client will accept
         });

        ics26Router.timeoutPacket(timeoutMsg);
        // commitment should be deleted
        bytes32 path = ICS24Host.packetCommitmentKeyCalldata(packet.sourceChannel, packet.sequence);
        bytes32 storedCommitment = ics26Router.IBC_STORE().getCommitment(path);
        assertEq(storedCommitment, 0);

        // transfer should be reverted
        uint256 senderBalanceAfterTimeout = erc20.balanceOf(sender);
        uint256 contractBalanceAfterTimeout = erc20.balanceOf(ics20Transfer.escrow());
        assertEq(senderBalanceAfterTimeout, transferAmount);
        assertEq(contractBalanceAfterTimeout, 0);
    }

    function test_success_timeoutNoop() public {
        IICS26RouterMsgs.Packet memory packet = _sendICS20TransferPacket();

        // make light client return timestamp that is after our timeout
        lightClient.setMembershipResult(packet.timeoutTimestamp + 1, false);

        IICS26RouterMsgs.MsgTimeoutPacket memory timeoutMsg = IICS26RouterMsgs.MsgTimeoutPacket({
            packet: packet,
            proofTimeout: bytes("doesntmatter"), // dummy client will accept
            proofHeight: IICS02ClientMsgs.Height({ revisionNumber: 1, revisionHeight: 42 }) // dummy client will accept
         });
        ics26Router.timeoutPacket(timeoutMsg);
        // commitment should be deleted
        bytes32 path = ICS24Host.packetCommitmentKeyCalldata(packet.sourceChannel, packet.sequence);
        bytes32 storedCommitment = ics26Router.IBC_STORE().getCommitment(path);
        assertEq(storedCommitment, 0);

        // call timeout again, should be noop
        vm.expectEmit();
        emit IICS26Router.Noop();
        ics26Router.timeoutPacket(timeoutMsg);
    }

    // This test case tests the scenario where IBCStore.deletePacketCommitment fails with custom error
    function test_failure_timeoutNoop() public {
        IICS26RouterMsgs.Packet memory packet = _sendICS20TransferPacket();

        // make light client return timestamp that is after our timeout
        lightClient.setMembershipResult(packet.timeoutTimestamp + 1, false);

        IICS26RouterMsgs.MsgTimeoutPacket memory timeoutMsg = IICS26RouterMsgs.MsgTimeoutPacket({
            packet: packet,
            proofTimeout: bytes("doesntmatter"), // dummy client will accept
            proofHeight: IICS02ClientMsgs.Height({ revisionNumber: 1, revisionHeight: 42 }) // dummy client will accept
         });
        ics26Router.timeoutPacket(timeoutMsg);
        // commitment should be deleted
        bytes32 path = ICS24Host.packetCommitmentKeyCalldata(packet.sourceChannel, packet.sequence);
        bytes32 storedCommitment = ics26Router.IBC_STORE().getCommitment(path);
        assertEq(storedCommitment, 0);

        // override IBCStore to ErroneousIBCStore
        vm.mockFunction(
            address(ics26Router.IBC_STORE()),
            address(new ErroneousIBCStore()),
            abi.encodeWithSelector(IIBCStore.deletePacketCommitment.selector)
        );
        // call timeout again, should throw CallFailure
        vm.expectRevert(abi.encodeWithSelector(ErroneousIBCStore.CallFailure.selector, "deletePacketCommitment"));
        ics26Router.timeoutPacket(timeoutMsg);
    }

    function test_success_receiveICS20PacketWithSourceDenom() public {
        IICS26RouterMsgs.Packet memory packet = _sendICS20TransferPacket();

        IICS26RouterMsgs.MsgAckPacket memory ackMsg = IICS26RouterMsgs.MsgAckPacket({
            packet: packet,
            acknowledgement: ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON,
            proofAcked: bytes("doesntmatter"), // dummy client will accept
            proofHeight: IICS02ClientMsgs.Height({ revisionNumber: 1, revisionHeight: 42 }) // dummy client will accept
         });

        ics26Router.ackPacket(ackMsg);

        // commitment should be deleted
        bytes32 path = ICS24Host.packetCommitmentKeyCalldata(packet.sourceChannel, packet.sequence);
        bytes32 storedCommitment = ics26Router.IBC_STORE().getCommitment(path);
        assertEq(storedCommitment, 0);

        uint256 senderBalanceAfterSend = erc20.balanceOf(sender);
        uint256 contractBalanceAfterSend = erc20.balanceOf(ics20Transfer.escrow());
        assertEq(senderBalanceAfterSend, 0);
        assertEq(contractBalanceAfterSend, transferAmount);

        // Return the tokens (receive)
        receiverStr = senderStr;
        receiver = sender;
        senderStr = "cosmos1mhmwgrfrcrdex5gnr0vcqt90wknunsxej63feh";
        string memory receivedDenom = string(abi.encodePacked("transfer/", counterpartyId, "/", erc20AddressStr));

        ICS20Lib.FungibleTokenPacketData memory receivePacketData = ICS20Lib.FungibleTokenPacketData({
            denom: receivedDenom,
            sender: senderStr,
            receiver: receiverStr,
            amount: transferAmount,
            memo: "backmemo"
        });

        IICS26RouterMsgs.Payload[] memory payloads = new IICS26RouterMsgs.Payload[](1);
        payloads[0] = IICS26RouterMsgs.Payload({
            sourcePort: ICS20Lib.DEFAULT_PORT_ID,
            destPort: ICS20Lib.DEFAULT_PORT_ID,
            version: ICS20Lib.ICS20_VERSION,
            encoding: ICS20Lib.ICS20_ENCODING,
            value: abi.encode(receivePacketData)
        });
        packet = IICS26RouterMsgs.Packet({
            sequence: 1,
            sourceChannel: counterpartyId,
            destChannel: clientIdentifier,
            timeoutTimestamp: packet.timeoutTimestamp + 1000,
            payloads: payloads
        });

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
        assertEq(erc20.balanceOf(ics20Transfer.escrow()), 0);

        // Check that the ack is written
        bytes32 storedAck = ics26Router.IBC_STORE().getCommitment(
            ICS24Host.packetAcknowledgementCommitmentKeyCalldata(packet.destChannel, packet.sequence)
        );
        assertEq(storedAck, ICS24Host.packetAcknowledgementCommitmentBytes32(singleSuccessAck));
    }

    function test_success_recvNoop() public {
        IICS26RouterMsgs.Packet memory packet = _sendICS20TransferPacket();

        IICS26RouterMsgs.MsgAckPacket memory ackMsg = IICS26RouterMsgs.MsgAckPacket({
            packet: packet,
            acknowledgement: ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON,
            proofAcked: bytes("doesntmatter"), // dummy client will accept
            proofHeight: IICS02ClientMsgs.Height({ revisionNumber: 1, revisionHeight: 42 }) // dummy client will accept
         });

        ics26Router.ackPacket(ackMsg);

        // commitment should be deleted
        bytes32 path = ICS24Host.packetCommitmentKeyCalldata(packet.sourceChannel, packet.sequence);
        bytes32 storedCommitment = ics26Router.IBC_STORE().getCommitment(path);
        assertEq(storedCommitment, 0);

        uint256 senderBalanceAfterSend = erc20.balanceOf(sender);
        uint256 contractBalanceAfterSend = erc20.balanceOf(ics20Transfer.escrow());
        assertEq(senderBalanceAfterSend, 0);
        assertEq(contractBalanceAfterSend, transferAmount);

        // Return the tokens (receive)
        receiverStr = senderStr;
        receiver = sender;
        senderStr = "cosmos1mhmwgrfrcrdex5gnr0vcqt90wknunsxej63feh";
        string memory receivedDenom = string(abi.encodePacked("transfer/", counterpartyId, "/", erc20AddressStr));

        ICS20Lib.FungibleTokenPacketData memory receivePacketData = ICS20Lib.FungibleTokenPacketData({
            denom: receivedDenom,
            sender: senderStr,
            receiver: receiverStr,
            amount: transferAmount,
            memo: "backmemo"
        });
        IICS26RouterMsgs.Payload[] memory payloads = new IICS26RouterMsgs.Payload[](1);
        payloads[0] = IICS26RouterMsgs.Payload({
            sourcePort: ICS20Lib.DEFAULT_PORT_ID,
            destPort: ICS20Lib.DEFAULT_PORT_ID,
            version: ICS20Lib.ICS20_VERSION,
            encoding: ICS20Lib.ICS20_ENCODING,
            value: abi.encode(receivePacketData)
        });
        packet = IICS26RouterMsgs.Packet({
            sequence: 1,
            sourceChannel: counterpartyId,
            destChannel: clientIdentifier,
            timeoutTimestamp: packet.timeoutTimestamp + 1000,
            payloads: payloads
        });

        IICS26RouterMsgs.MsgRecvPacket memory msgRecvPacket = IICS26RouterMsgs.MsgRecvPacket({
            packet: packet,
            proofCommitment: bytes("doesntmatter"), // dummy client will accept
            proofHeight: IICS02ClientMsgs.Height({ revisionNumber: 1, revisionHeight: 42 }) // will accept
         });
        ics26Router.recvPacket(msgRecvPacket);

        // Check that the ack is written
        bytes32 storedAck = ics26Router.IBC_STORE().getCommitment(
            ICS24Host.packetAcknowledgementCommitmentKeyCalldata(packet.destChannel, packet.sequence)
        );
        assertEq(storedAck, ICS24Host.packetAcknowledgementCommitmentBytes32(singleSuccessAck));

        // call recv again, should be noop
        vm.expectEmit();
        emit IICS26Router.Noop();
        ics26Router.recvPacket(msgRecvPacket);
    }

    // This test case tests the scenario where IBCStore.setPacketReceipt fails with custom error
    function test_failure_recvNoop() public {
        IICS26RouterMsgs.Packet memory packet = _sendICS20TransferPacket();

        IICS26RouterMsgs.MsgAckPacket memory ackMsg = IICS26RouterMsgs.MsgAckPacket({
            packet: packet,
            acknowledgement: ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON,
            proofAcked: bytes("doesntmatter"), // dummy client will accept
            proofHeight: IICS02ClientMsgs.Height({ revisionNumber: 1, revisionHeight: 42 }) // dummy client will accept
         });

        ics26Router.ackPacket(ackMsg);

        // commitment should be deleted
        bytes32 path = ICS24Host.packetCommitmentKeyCalldata(packet.sourceChannel, packet.sequence);
        bytes32 storedCommitment = ics26Router.IBC_STORE().getCommitment(path);
        assertEq(storedCommitment, 0);

        uint256 senderBalanceAfterSend = erc20.balanceOf(sender);
        uint256 contractBalanceAfterSend = erc20.balanceOf(ics20Transfer.escrow());
        assertEq(senderBalanceAfterSend, 0);
        assertEq(contractBalanceAfterSend, transferAmount);

        // Return the tokens (receive)
        receiverStr = senderStr;
        receiver = sender;
        senderStr = "cosmos1mhmwgrfrcrdex5gnr0vcqt90wknunsxej63feh";
        string memory receivedDenom = string(abi.encodePacked("transfer/", counterpartyId, "/", erc20AddressStr));

        ICS20Lib.FungibleTokenPacketData memory receivePacketData = ICS20Lib.FungibleTokenPacketData({
            denom: receivedDenom,
            sender: senderStr,
            receiver: receiverStr,
            amount: transferAmount,
            memo: "backmemo"
        });
        IICS26RouterMsgs.Payload[] memory payloads = new IICS26RouterMsgs.Payload[](1);
        payloads[0] = IICS26RouterMsgs.Payload({
            sourcePort: ICS20Lib.DEFAULT_PORT_ID,
            destPort: ICS20Lib.DEFAULT_PORT_ID,
            version: ICS20Lib.ICS20_VERSION,
            encoding: ICS20Lib.ICS20_ENCODING,
            value: abi.encode(receivePacketData)
        });
        packet = IICS26RouterMsgs.Packet({
            sequence: 1,
            sourceChannel: counterpartyId,
            destChannel: clientIdentifier,
            timeoutTimestamp: packet.timeoutTimestamp + 1000,
            payloads: payloads
        });

        IICS26RouterMsgs.MsgRecvPacket memory msgRecvPacket = IICS26RouterMsgs.MsgRecvPacket({
            packet: packet,
            proofCommitment: bytes("doesntmatter"), // dummy client will accept
            proofHeight: IICS02ClientMsgs.Height({ revisionNumber: 1, revisionHeight: 42 }) // will accept
         });
        ics26Router.recvPacket(msgRecvPacket);

        // Check that the ack is written
        bytes32 storedAck = ics26Router.IBC_STORE().getCommitment(
            ICS24Host.packetAcknowledgementCommitmentKeyCalldata(packet.destChannel, packet.sequence)
        );
        assertEq(storedAck, ICS24Host.packetAcknowledgementCommitmentBytes32(singleSuccessAck));

        // override IBCStore to ErroneousIBCStore
        vm.mockFunction(
            address(ics26Router.IBC_STORE()),
            address(new ErroneousIBCStore()),
            abi.encodeWithSelector(IIBCStore.setPacketReceipt.selector)
        );
        // call recv again, should throw CallFailure
        vm.expectRevert(abi.encodeWithSelector(ErroneousIBCStore.CallFailure.selector, "setPacketReceipt"));
        ics26Router.recvPacket(msgRecvPacket);
    }

    function test_success_receiveICS20PacketWithForeignBaseDenom() public {
        string memory foreignDenom = "uatom";

        senderStr = "cosmos1mhmwgrfrcrdex5gnr0vcqt90wknunsxej63feh";
        receiver = makeAddr("receiver_of_foreign_denom");
        receiverStr = Strings.toHexString(receiver);

        ICS20Lib.FungibleTokenPacketData memory receivePacketData = ICS20Lib.FungibleTokenPacketData({
            denom: foreignDenom,
            sender: senderStr,
            receiver: receiverStr,
            amount: transferAmount,
            memo: "memo"
        });

        IICS26RouterMsgs.Payload[] memory payloads = new IICS26RouterMsgs.Payload[](1);
        payloads[0] = IICS26RouterMsgs.Payload({
            sourcePort: ICS20Lib.DEFAULT_PORT_ID,
            destPort: ICS20Lib.DEFAULT_PORT_ID,
            version: ICS20Lib.ICS20_VERSION,
            encoding: ICS20Lib.ICS20_ENCODING,
            value: abi.encode(receivePacketData)
        });
        IICS26RouterMsgs.Packet memory receivePacket = IICS26RouterMsgs.Packet({
            sequence: 1,
            sourceChannel: counterpartyId,
            destChannel: clientIdentifier,
            timeoutTimestamp: uint64(block.timestamp + 1000),
            payloads: payloads
        });

        string memory expectedFullDenomPath = string(
            abi.encodePacked(receivePacket.payloads[0].destPort, "/", receivePacket.destChannel, "/", foreignDenom)
        );

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
        bytes32 storedAck = ics26Router.IBC_STORE().getCommitment(
            ICS24Host.packetAcknowledgementCommitmentKeyCalldata(receivePacket.destChannel, receivePacket.sequence)
        );
        assertEq(storedAck, ICS24Host.packetAcknowledgementCommitmentBytes32(singleSuccessAck));

        address erc20Address = address(ics20Transfer.ibcERC20Contract(ICS20Lib.toIBCDenom(expectedFullDenomPath)));

        ICS20Lib.FungibleTokenPacketData memory packetData =
            abi.decode(receivePacket.payloads[0].value, (ICS20Lib.FungibleTokenPacketData));

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
            destPort: ICS20Lib.DEFAULT_PORT_ID,
            timeoutTimestamp: uint64(block.timestamp + 1000),
            memo: "backmemo"
        });

        ICS20Lib.FungibleTokenPacketData memory sendPacketData = ICS20Lib.FungibleTokenPacketData({
            denom: expectedFullDenomPath,
            sender: senderStr,
            receiver: receiverStr,
            amount: transferAmount,
            memo: "backmemo"
        });
        IICS26RouterMsgs.Payload[] memory expectedPayloads = new IICS26RouterMsgs.Payload[](1);
        expectedPayloads[0] = IICS26RouterMsgs.Payload({
            sourcePort: ICS20Lib.DEFAULT_PORT_ID,
            destPort: ICS20Lib.DEFAULT_PORT_ID,
            version: ICS20Lib.ICS20_VERSION,
            encoding: ICS20Lib.ICS20_ENCODING,
            value: abi.encode(sendPacketData)
        });
        IICS26RouterMsgs.Packet memory expectedPacketSent = IICS26RouterMsgs.Packet({
            sequence: 1,
            sourceChannel: clientIdentifier,
            destChannel: counterpartyId,
            timeoutTimestamp: msgSendTransfer.timeoutTimestamp,
            payloads: expectedPayloads
        });
        vm.expectEmit();
        emit IICS26Router.SendPacket(expectedPacketSent);
        vm.prank(sender);
        uint32 sequence = ics20Transfer.sendTransfer(msgSendTransfer);
        assertEq(sequence, expectedPacketSent.sequence);

        assertEq(ibcERC20.totalSupply(), 0);
        assertEq(ibcERC20.balanceOf(sender), 0);

        bytes32 path =
            ICS24Host.packetCommitmentKeyCalldata(expectedPacketSent.sourceChannel, expectedPacketSent.sequence);
        bytes32 storedCommitment = ics26Router.IBC_STORE().getCommitment(path);
        assertEq(storedCommitment, ICS24Host.packetCommitmentBytes32(expectedPacketSent));
    }

    function test_success_receiveMultiPacketWithForeignBaseDenom() public {
        string memory foreignDenom = "uatom";

        senderStr = "cosmos1mhmwgrfrcrdex5gnr0vcqt90wknunsxej63feh";
        receiver = makeAddr("receiver_of_foreign_denom");
        receiverStr = Strings.toHexString(receiver);

        // First packet
        ICS20Lib.FungibleTokenPacketData memory packetData = ICS20Lib.FungibleTokenPacketData({
            denom: foreignDenom,
            amount: transferAmount,
            sender: senderStr,
            receiver: receiverStr,
            memo: "memo"
        });
        IICS26RouterMsgs.Payload[] memory payloads1 = new IICS26RouterMsgs.Payload[](1);
        payloads1[0] = IICS26RouterMsgs.Payload({
            sourcePort: ICS20Lib.DEFAULT_PORT_ID,
            destPort: ICS20Lib.DEFAULT_PORT_ID,
            version: ICS20Lib.ICS20_VERSION,
            encoding: ICS20Lib.ICS20_ENCODING,
            value: abi.encode(packetData)
        });
        IICS26RouterMsgs.Packet memory receivePacket = IICS26RouterMsgs.Packet({
            sequence: 1,
            sourceChannel: counterpartyId,
            destChannel: clientIdentifier,
            timeoutTimestamp: uint64(block.timestamp + 1000),
            payloads: payloads1
        });

        // Second packet
        IICS26RouterMsgs.Payload[] memory payloads2 = new IICS26RouterMsgs.Payload[](1);
        payloads2[0] = IICS26RouterMsgs.Payload({
            sourcePort: ICS20Lib.DEFAULT_PORT_ID,
            destPort: ICS20Lib.DEFAULT_PORT_ID,
            version: ICS20Lib.ICS20_VERSION,
            encoding: ICS20Lib.ICS20_ENCODING,
            value: abi.encode(packetData)
        });
        IICS26RouterMsgs.Packet memory receivePacket2 = IICS26RouterMsgs.Packet({
            sequence: 2,
            sourceChannel: counterpartyId,
            destChannel: clientIdentifier,
            timeoutTimestamp: uint64(block.timestamp + 1000),
            payloads: payloads2
        });

        bytes[] memory multicallData = new bytes[](2);
        multicallData[0] = abi.encodeCall(
            IICS26Router.recvPacket,
            IICS26RouterMsgs.MsgRecvPacket({
                packet: receivePacket,
                proofCommitment: bytes("doesntmatter"), // dummy client will accept
                proofHeight: IICS02ClientMsgs.Height({ revisionNumber: 1, revisionHeight: 42 }) // will accept
             })
        );
        multicallData[1] = abi.encodeCall(
            IICS26Router.recvPacket,
            IICS26RouterMsgs.MsgRecvPacket({
                packet: receivePacket2,
                proofCommitment: bytes("doesntmatter"), // dummy client will accept
                proofHeight: IICS02ClientMsgs.Height({ revisionNumber: 1, revisionHeight: 42 }) // will accept
             })
        );

        ics26Router.multicall(multicallData);

        // Check that the ack is written
        bytes32 storedAck = ics26Router.IBC_STORE().getCommitment(
            ICS24Host.packetAcknowledgementCommitmentKeyCalldata(receivePacket.destChannel, receivePacket.sequence)
        );
        assertEq(storedAck, ICS24Host.packetAcknowledgementCommitmentBytes32(singleSuccessAck));
        bytes32 storedAck2 = ics26Router.IBC_STORE().getCommitment(
            ICS24Host.packetAcknowledgementCommitmentKeyCalldata(receivePacket2.destChannel, receivePacket2.sequence)
        );
        assertEq(storedAck2, ICS24Host.packetAcknowledgementCommitmentBytes32(singleSuccessAck));
    }

    function test_failure_receiveMultiPacketWithForeignBaseDenom() public {
        string memory foreignDenom = "uatom";

        senderStr = "cosmos1mhmwgrfrcrdex5gnr0vcqt90wknunsxej63feh";
        receiver = makeAddr("receiver_of_foreign_denom");
        receiverStr = Strings.toHexString(receiver);

        // First packet
        ICS20Lib.FungibleTokenPacketData memory packetData = ICS20Lib.FungibleTokenPacketData({
            denom: foreignDenom,
            amount: transferAmount,
            sender: senderStr,
            receiver: receiverStr,
            memo: "memo"
        });

        IICS26RouterMsgs.Payload[] memory payloads1 = new IICS26RouterMsgs.Payload[](1);
        payloads1[0] = IICS26RouterMsgs.Payload({
            sourcePort: ICS20Lib.DEFAULT_PORT_ID,
            destPort: ICS20Lib.DEFAULT_PORT_ID,
            version: ICS20Lib.ICS20_VERSION,
            encoding: ICS20Lib.ICS20_ENCODING,
            value: abi.encode(packetData)
        });
        IICS26RouterMsgs.Packet memory receivePacket = IICS26RouterMsgs.Packet({
            sequence: 1,
            sourceChannel: counterpartyId,
            destChannel: clientIdentifier,
            timeoutTimestamp: uint64(block.timestamp + 1000),
            payloads: payloads1
        });

        // Second packet
        IICS26RouterMsgs.Payload[] memory payloads2 = new IICS26RouterMsgs.Payload[](1);
        payloads2[0] = IICS26RouterMsgs.Payload({
            sourcePort: ICS20Lib.DEFAULT_PORT_ID,
            destPort: "invalid-port",
            version: ICS20Lib.ICS20_VERSION,
            encoding: ICS20Lib.ICS20_ENCODING,
            value: abi.encode(packetData)
        });
        IICS26RouterMsgs.Packet memory invalidPacket = IICS26RouterMsgs.Packet({
            sequence: 2,
            sourceChannel: counterpartyId,
            destChannel: clientIdentifier,
            timeoutTimestamp: uint64(block.timestamp + 1000),
            payloads: payloads2
        });

        bytes[] memory multicallData = new bytes[](2);
        multicallData[0] = abi.encodeCall(
            IICS26Router.recvPacket,
            IICS26RouterMsgs.MsgRecvPacket({
                packet: receivePacket,
                proofCommitment: bytes("doesntmatter"), // dummy client will accept
                proofHeight: IICS02ClientMsgs.Height({ revisionNumber: 1, revisionHeight: 42 }) // will accept
             })
        );
        multicallData[1] = abi.encodeCall(
            IICS26Router.recvPacket,
            IICS26RouterMsgs.MsgRecvPacket({
                packet: invalidPacket,
                proofCommitment: bytes("doesntmatter"), // dummy client will accept
                proofHeight: IICS02ClientMsgs.Height({ revisionNumber: 1, revisionHeight: 42 }) // will accept
             })
        );

        vm.expectRevert(
            abi.encodeWithSelector(IICS26RouterErrors.IBCAppNotFound.selector, invalidPacket.payloads[0].destPort)
        );
        ics26Router.multicall(multicallData);
    }

    function test_success_receiveICS20PacketWithForeignIBCDenom() public {
        string memory foreignDenom = "transfer/channel-42/uatom";

        senderStr = "cosmos1mhmwgrfrcrdex5gnr0vcqt90wknunsxej63feh";
        receiver = makeAddr("receiver_of_foreign_denom");
        receiverStr = Strings.toHexString(receiver);

        ICS20Lib.FungibleTokenPacketData memory receivePacketData = ICS20Lib.FungibleTokenPacketData({
            denom: foreignDenom,
            amount: transferAmount,
            sender: senderStr,
            receiver: receiverStr,
            memo: "memo"
        });
        IICS26RouterMsgs.Payload[] memory receievePayloads = new IICS26RouterMsgs.Payload[](1);
        receievePayloads[0] = IICS26RouterMsgs.Payload({
            sourcePort: ICS20Lib.DEFAULT_PORT_ID,
            destPort: ICS20Lib.DEFAULT_PORT_ID,
            version: ICS20Lib.ICS20_VERSION,
            encoding: ICS20Lib.ICS20_ENCODING,
            value: abi.encode(receivePacketData)
        });
        IICS26RouterMsgs.Packet memory receivePacket = IICS26RouterMsgs.Packet({
            sequence: 1,
            sourceChannel: counterpartyId,
            destChannel: clientIdentifier,
            timeoutTimestamp: uint64(block.timestamp + 1000),
            payloads: receievePayloads
        });

        string memory expectedFullDenomPath = string(
            abi.encodePacked(receivePacket.payloads[0].destPort, "/", receivePacket.destChannel, "/", foreignDenom)
        );

        vm.expectEmit();
        emit IICS26Router.RecvPacket(receivePacket);

        ics26Router.recvPacket(
            IICS26RouterMsgs.MsgRecvPacket({
                packet: receivePacket,
                proofCommitment: bytes("doesntmatter"), // dummy client will accept
                proofHeight: IICS02ClientMsgs.Height({ revisionNumber: 1, revisionHeight: 42 }) // will accept
             })
        );

        // Check that the ack is written
        bytes32 storedAck = ics26Router.IBC_STORE().getCommitment(
            ICS24Host.packetAcknowledgementCommitmentKeyCalldata(receivePacket.destChannel, receivePacket.sequence)
        );
        assertEq(storedAck, ICS24Host.packetAcknowledgementCommitmentBytes32(singleSuccessAck));

        address erc20Address = address(ics20Transfer.ibcERC20Contract(ICS20Lib.toIBCDenom(expectedFullDenomPath)));

        ICS20Lib.FungibleTokenPacketData memory packetData =
            abi.decode(receivePacket.payloads[0].value, (ICS20Lib.FungibleTokenPacketData));

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
            destPort: ICS20Lib.DEFAULT_PORT_ID,
            timeoutTimestamp: uint64(block.timestamp + 1000),
            memo: "backmemo"
        });

        vm.expectEmit();

        ICS20Lib.FungibleTokenPacketData memory sendPacketData = ICS20Lib.FungibleTokenPacketData({
            denom: expectedFullDenomPath,
            sender: senderStr,
            receiver: receiverStr,
            amount: transferAmount,
            memo: "backmemo"
        });
        IICS26RouterMsgs.Payload[] memory expectedPayloads = new IICS26RouterMsgs.Payload[](1);
        expectedPayloads[0] = IICS26RouterMsgs.Payload({
            sourcePort: ICS20Lib.DEFAULT_PORT_ID,
            destPort: ICS20Lib.DEFAULT_PORT_ID,
            version: ICS20Lib.ICS20_VERSION,
            encoding: ICS20Lib.ICS20_ENCODING,
            value: abi.encode(sendPacketData)
        });
        IICS26RouterMsgs.Packet memory expectedPacketSent = IICS26RouterMsgs.Packet({
            sequence: 1,
            sourceChannel: clientIdentifier,
            destChannel: counterpartyId,
            timeoutTimestamp: msgSendTransfer.timeoutTimestamp,
            payloads: expectedPayloads
        });
        emit IICS26Router.SendPacket(expectedPacketSent);

        vm.prank(sender);
        uint32 sequence = ics20Transfer.sendTransfer(msgSendTransfer);

        assertEq(sequence, expectedPacketSent.sequence);

        assertEq(ibcERC20.totalSupply(), 0);
        assertEq(ibcERC20.balanceOf(receiver), 0);

        bytes32 path =
            ICS24Host.packetCommitmentKeyCalldata(expectedPacketSent.sourceChannel, expectedPacketSent.sequence);
        bytes32 storedCommitment = ics26Router.IBC_STORE().getCommitment(path);
        assertEq(storedCommitment, ICS24Host.packetCommitmentBytes32(expectedPacketSent));
    }

    function test_success_receiveICS20PacketWithLargeAmountAndForeignIBCDenom() public {
        string memory foreignDenom = "transfer/channel-42/uatom";

        senderStr = "cosmos1mhmwgrfrcrdex5gnr0vcqt90wknunsxej63feh";
        receiver = makeAddr("receiver_of_foreign_denom");
        receiverStr = Strings.toHexString(receiver);

        uint256 largeAmount = 1_000_000_000_000_000_001;

        ICS20Lib.FungibleTokenPacketData memory receivePacketData = ICS20Lib.FungibleTokenPacketData({
            denom: foreignDenom,
            amount: largeAmount,
            sender: senderStr,
            receiver: receiverStr,
            memo: ""
        });
        IICS26RouterMsgs.Payload[] memory payloads = new IICS26RouterMsgs.Payload[](1);
        payloads[0] = IICS26RouterMsgs.Payload({
            sourcePort: ICS20Lib.DEFAULT_PORT_ID,
            destPort: ICS20Lib.DEFAULT_PORT_ID,
            version: ICS20Lib.ICS20_VERSION,
            encoding: ICS20Lib.ICS20_ENCODING,
            value: abi.encode(receivePacketData)
        });
        IICS26RouterMsgs.Packet memory receivePacket = IICS26RouterMsgs.Packet({
            sequence: 1,
            sourceChannel: counterpartyId,
            destChannel: clientIdentifier,
            timeoutTimestamp: uint64(block.timestamp + 1000),
            payloads: payloads
        });

        string memory expectedFullDenomPath = string(
            abi.encodePacked(receivePacket.payloads[0].destPort, "/", receivePacket.destChannel, "/", foreignDenom)
        );

        vm.expectEmit();
        emit IICS26Router.RecvPacket(receivePacket);

        ics26Router.recvPacket(
            IICS26RouterMsgs.MsgRecvPacket({
                packet: receivePacket,
                proofCommitment: bytes("doesntmatter"), // dummy client will accept
                proofHeight: IICS02ClientMsgs.Height({ revisionNumber: 1, revisionHeight: 42 }) // will accept
             })
        );

        // Check that the ack is written
        bytes32 storedAck = ics26Router.IBC_STORE().getCommitment(
            ICS24Host.packetAcknowledgementCommitmentKeyCalldata(receivePacket.destChannel, receivePacket.sequence)
        );
        assertEq(storedAck, ICS24Host.packetAcknowledgementCommitmentBytes32(singleSuccessAck));

        address erc20Address = address(ics20Transfer.ibcERC20Contract(ICS20Lib.toIBCDenom(expectedFullDenomPath)));

        ICS20Lib.FungibleTokenPacketData memory packetData =
            abi.decode(receivePacket.payloads[0].value, (ICS20Lib.FungibleTokenPacketData));

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
            destPort: ICS20Lib.DEFAULT_PORT_ID,
            timeoutTimestamp: uint64(block.timestamp + 1000),
            memo: ""
        });

        vm.expectEmit();

        ICS20Lib.FungibleTokenPacketData memory sendPacketData = ICS20Lib.FungibleTokenPacketData({
            denom: expectedFullDenomPath,
            sender: senderStr,
            receiver: receiverStr,
            amount: largeAmount,
            memo: ""
        });
        IICS26RouterMsgs.Payload[] memory expectedPayloads = new IICS26RouterMsgs.Payload[](1);
        expectedPayloads[0] = IICS26RouterMsgs.Payload({
            sourcePort: ICS20Lib.DEFAULT_PORT_ID,
            destPort: ICS20Lib.DEFAULT_PORT_ID,
            version: ICS20Lib.ICS20_VERSION,
            encoding: ICS20Lib.ICS20_ENCODING,
            value: abi.encode(sendPacketData)
        });
        IICS26RouterMsgs.Packet memory expectedPacketSent = IICS26RouterMsgs.Packet({
            sequence: 1,
            sourceChannel: clientIdentifier,
            destChannel: counterpartyId,
            timeoutTimestamp: msgSendTransfer.timeoutTimestamp,
            payloads: expectedPayloads
        });
        emit IICS26Router.SendPacket(expectedPacketSent);

        vm.prank(sender);
        uint32 sequence = ics20Transfer.sendTransfer(msgSendTransfer);

        assertEq(sequence, expectedPacketSent.sequence);

        assertEq(ibcERC20.totalSupply(), 0);
        assertEq(ibcERC20.balanceOf(receiver), 0);

        bytes32 path =
            ICS24Host.packetCommitmentKeyCalldata(expectedPacketSent.sourceChannel, expectedPacketSent.sequence);
        bytes32 storedCommitment = ics26Router.IBC_STORE().getCommitment(path);
        assertEq(storedCommitment, ICS24Host.packetCommitmentBytes32(expectedPacketSent));
    }

    function test_failure_receiveICS20PacketHasTimedOut() public {
        IICS26RouterMsgs.Packet memory packet = _sendICS20TransferPacket();

        IICS26RouterMsgs.MsgAckPacket memory ackMsg = IICS26RouterMsgs.MsgAckPacket({
            packet: packet,
            acknowledgement: ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON,
            proofAcked: bytes("doesntmatter"), // dummy client will accept
            proofHeight: IICS02ClientMsgs.Height({ revisionNumber: 1, revisionHeight: 42 }) // dummy client will accept
         });

        ics26Router.ackPacket(ackMsg);

        // commitment should be deleted
        bytes32 path = ICS24Host.packetCommitmentKeyCalldata(packet.sourceChannel, packet.sequence);
        bytes32 storedCommitment = ics26Router.IBC_STORE().getCommitment(path);
        assertEq(storedCommitment, 0);

        uint256 senderBalanceAfterSend = erc20.balanceOf(sender);
        uint256 contractBalanceAfterSend = erc20.balanceOf(ics20Transfer.escrow());
        assertEq(senderBalanceAfterSend, 0);
        assertEq(contractBalanceAfterSend, transferAmount);

        // Send back
        receiverStr = senderStr;
        senderStr = "cosmos1mhmwgrfrcrdex5gnr0vcqt90wknunsxej63feh";
        string memory ibcDenom = string(abi.encodePacked("transfer/", counterpartyId, "/", erc20AddressStr));

        ICS20Lib.FungibleTokenPacketData memory receivePacketData = ICS20Lib.FungibleTokenPacketData({
            denom: ibcDenom,
            sender: senderStr,
            receiver: receiverStr,
            amount: transferAmount,
            memo: "backmemo"
        });
        uint64 timeoutTimestamp = uint64(block.timestamp - 1);
        IICS26RouterMsgs.Payload[] memory payloads = new IICS26RouterMsgs.Payload[](1);
        payloads[0] = IICS26RouterMsgs.Payload({
            sourcePort: ICS20Lib.DEFAULT_PORT_ID,
            destPort: ICS20Lib.DEFAULT_PORT_ID,
            version: ICS20Lib.ICS20_VERSION,
            encoding: ICS20Lib.ICS20_ENCODING,
            value: abi.encode(receivePacketData)
        });
        packet = IICS26RouterMsgs.Packet({
            sequence: 1,
            sourceChannel: counterpartyId,
            destChannel: clientIdentifier,
            timeoutTimestamp: timeoutTimestamp,
            payloads: payloads
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

    function _sendICS20TransferPacket() internal returns (IICS26RouterMsgs.Packet memory) {
        erc20.mint(sender, transferAmount);
        vm.startPrank(sender);
        erc20.approve(address(ics20Transfer), transferAmount);

        uint256 senderBalanceBefore = erc20.balanceOf(sender);
        uint256 contractBalanceBefore = erc20.balanceOf(ics20Transfer.escrow());
        assertEq(senderBalanceBefore, transferAmount);
        assertEq(contractBalanceBefore, 0);

        uint64 timeoutTimestamp = uint64(block.timestamp + 1000);
        IICS26RouterMsgs.MsgSendPacket memory msgSendPacket = ICS20Lib.newMsgSendPacketV1(
            sender,
            IICS20TransferMsgs.SendTransferMsg({
                denom: erc20AddressStr,
                amount: transferAmount,
                receiver: receiverStr,
                sourceChannel: clientIdentifier,
                destPort: ICS20Lib.DEFAULT_PORT_ID,
                timeoutTimestamp: timeoutTimestamp,
                memo: "memo"
            })
        );

        uint32 sequence = ics26Router.sendPacket(msgSendPacket);
        assertEq(sequence, 1);

        IICS26RouterMsgs.Payload[] memory packetPayloads = new IICS26RouterMsgs.Payload[](1);
        packetPayloads[0] = IICS26RouterMsgs.Payload({
            sourcePort: ICS20Lib.DEFAULT_PORT_ID,
            destPort: ICS20Lib.DEFAULT_PORT_ID,
            version: ICS20Lib.ICS20_VERSION,
            encoding: ICS20Lib.ICS20_ENCODING,
            value: abi.encode(expectedDefaultSendPacketData)
        });
        IICS26RouterMsgs.Packet memory packet = IICS26RouterMsgs.Packet({
            sequence: sequence,
            sourceChannel: msgSendPacket.sourceChannel,
            destChannel: counterpartyId,
            timeoutTimestamp: timeoutTimestamp,
            payloads: packetPayloads
        });

        bytes32 path = ICS24Host.packetCommitmentKeyCalldata(msgSendPacket.sourceChannel, sequence);
        bytes32 storedCommitment = ics26Router.IBC_STORE().getCommitment(path);
        assertEq(storedCommitment, ICS24Host.packetCommitmentBytes32(packet));

        return packet;
    }
}
