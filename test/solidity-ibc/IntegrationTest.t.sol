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
import { IICS26RouterErrors } from "../../contracts/errors/IICS26RouterErrors.sol";
import { ICS26Router } from "../../contracts/ICS26Router.sol";
import { IICS26RouterMsgs } from "../../contracts/msgs/IICS26RouterMsgs.sol";
import { DummyLightClient } from "./mocks/DummyLightClient.sol";
import { ILightClientMsgs } from "../../contracts/msgs/ILightClientMsgs.sol";
import { ICS20Lib } from "../../contracts/utils/ICS20Lib.sol";
import { ICS24Host } from "../../contracts/utils/ICS24Host.sol";
import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";
import { ERC1967Proxy } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Proxy.sol";

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

    function setUp() public {
        // ============ Step 1: Deploy the logic contracts ==============
        lightClient = new DummyLightClient(ILightClientMsgs.UpdateResult.Update, 0, false);
        ICS26Router ics26RouterLogic = new ICS26Router();
        ICS20Transfer ics20TransferLogic = new ICS20Transfer();

        // ============== Step 2: Deploy Transparent Proxies ==============
        ERC1967Proxy routerProxy = new ERC1967Proxy(
            address(ics26RouterLogic),
            abi.encodeWithSelector(ICS26Router.initialize.selector, address(this), address(this))
        );

        ERC1967Proxy transferProxy = new ERC1967Proxy(
            address(ics20TransferLogic), abi.encodeWithSelector(ICS20Transfer.initialize.selector, address(routerProxy))
        );

        // ============== Step 3: Wire up the contracts ==============
        ics26Router = ICS26Router(address(routerProxy));
        ics20Transfer = ICS20Transfer(address(transferProxy));
        erc20 = new TestERC20();
        erc20AddressStr = Strings.toHexString(address(erc20));

        clientIdentifier = ics26Router.addClient(
            "07-tendermint", IICS02ClientMsgs.CounterpartyInfo(counterpartyId, merklePrefix), address(lightClient)
        );
        ics20AddressStr = Strings.toHexString(address(ics20Transfer));

        vm.expectEmit();
        emit IICS26Router.IBCAppAdded(ICS20Lib.DEFAULT_PORT_ID, address(ics20Transfer));
        ics26Router.addIBCApp(ICS20Lib.DEFAULT_PORT_ID, address(ics20Transfer));

        assertEq(address(ics20Transfer), address(ics26Router.getIBCApp(ICS20Lib.DEFAULT_PORT_ID)));

        sender = makeAddr("sender");
        senderStr = Strings.toHexString(sender);
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
        bytes32 path = ICS24Host.packetCommitmentKeyCalldata(packet.sourceClient, packet.sequence);
        bytes32 storedCommitment = ics26Router.getCommitment(path);
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

        IICS20TransferMsgs.ERC20Token[] memory defaultSendTransferMsgTokens = _getDefaultSendTransferMsgTokens();
        IICS20TransferMsgs.SendTransferMsg memory transferMsg = IICS20TransferMsgs.SendTransferMsg({
            tokens: defaultSendTransferMsgTokens,
            receiver: receiverStr,
            sourceClient: clientIdentifier,
            destPort: ICS20Lib.DEFAULT_PORT_ID,
            timeoutTimestamp: uint64(block.timestamp + 1000),
            memo: "memo",
            forwarding: IICS20TransferMsgs.Forwarding({ hops: new IICS20TransferMsgs.Hop[](0) })
        });

        vm.startPrank(sender);
        uint32 sequence = ics20Transfer.sendTransfer(transferMsg);
        assertEq(sequence, 1);

        IICS20TransferMsgs.FungibleTokenPacketDataV2 memory packetData = _getDefaultPacketData();

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
            sourceClient: transferMsg.sourceClient,
            destClient: counterpartyId, // If we test with something else, we need to add this to the args
            timeoutTimestamp: transferMsg.timeoutTimestamp,
            payloads: packetPayloads
        });

        bytes32 path = ICS24Host.packetCommitmentKeyCalldata(transferMsg.sourceClient, sequence);
        bytes32 storedCommitment = ics26Router.getCommitment(path);
        assertEq(storedCommitment, ICS24Host.packetCommitmentBytes32(packet));

        IICS26RouterMsgs.MsgAckPacket memory ackMsg = IICS26RouterMsgs.MsgAckPacket({
            packet: packet,
            acknowledgement: ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON,
            proofAcked: bytes("doesntmatter"), // dummy client will accept
            proofHeight: IICS02ClientMsgs.Height({ revisionNumber: 1, revisionHeight: 42 }) // dummy client will accept
         });

        ics26Router.ackPacket(ackMsg);

        // commitment should be deleted
        storedCommitment = ics26Router.getCommitment(path);
        assertEq(storedCommitment, 0);

        uint256 senderBalanceAfter = erc20.balanceOf(sender);
        uint256 contractBalanceAfter = erc20.balanceOf(ics20Transfer.escrow());
        assertEq(senderBalanceAfter, 0);
        assertEq(contractBalanceAfter, transferAmount);
    }

    function test_failure_sendPacketWithLargeTimeoutDuration() public {
        uint64 timeoutTimestamp = uint64(block.timestamp + 2 days);
        IICS20TransferMsgs.ERC20Token[] memory defaultSendTransferMsgTokens = _getDefaultSendTransferMsgTokens();
        IICS26RouterMsgs.MsgSendPacket memory msgSendPacket = ICS20Lib.newMsgSendPacketV2(
            sender,
            IICS20TransferMsgs.SendTransferMsg({
                tokens: defaultSendTransferMsgTokens,
                receiver: receiverStr,
                sourceClient: clientIdentifier,
                destPort: ICS20Lib.DEFAULT_PORT_ID,
                timeoutTimestamp: timeoutTimestamp,
                memo: "memo",
                forwarding: IICS20TransferMsgs.Forwarding({ hops: new IICS20TransferMsgs.Hop[](0) })
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
        bytes32 path = ICS24Host.packetCommitmentKeyCalldata(packet.sourceClient, packet.sequence);
        bytes32 storedCommitment = ics26Router.getCommitment(path);
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
        bytes32 path = ICS24Host.packetCommitmentKeyCalldata(packet.sourceClient, packet.sequence);
        bytes32 storedCommitment = ics26Router.getCommitment(path);
        assertEq(storedCommitment, 0);

        // call ack again, should be noop
        vm.expectEmit();
        emit IICS26Router.Noop();
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
        bytes32 path = ICS24Host.packetCommitmentKeyCalldata(packet.sourceClient, packet.sequence);
        bytes32 storedCommitment = ics26Router.getCommitment(path);
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
        bytes32 path = ICS24Host.packetCommitmentKeyCalldata(packet.sourceClient, packet.sequence);
        bytes32 storedCommitment = ics26Router.getCommitment(path);
        assertEq(storedCommitment, 0);

        // call timeout again, should be noop
        vm.expectEmit();
        emit IICS26Router.Noop();
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
        bytes32 path = ICS24Host.packetCommitmentKeyCalldata(packet.sourceClient, packet.sequence);
        bytes32 storedCommitment = ics26Router.getCommitment(path);
        assertEq(storedCommitment, 0);

        uint256 senderBalanceAfterSend = erc20.balanceOf(sender);
        uint256 contractBalanceAfterSend = erc20.balanceOf(ics20Transfer.escrow());
        assertEq(senderBalanceAfterSend, 0);
        assertEq(contractBalanceAfterSend, transferAmount);

        // Return the tokens (receive)
        receiverStr = senderStr;
        receiver = sender;
        senderStr = "cosmos1mhmwgrfrcrdex5gnr0vcqt90wknunsxej63feh";
        IICS20TransferMsgs.Denom memory receivedDenom =
            IICS20TransferMsgs.Denom({ base: erc20AddressStr, trace: new IICS20TransferMsgs.Hop[](1) });
        receivedDenom.trace[0] = IICS20TransferMsgs.Hop({ portId: ICS20Lib.DEFAULT_PORT_ID, clientId: counterpartyId });
        IICS20TransferMsgs.Token[] memory tokens = new IICS20TransferMsgs.Token[](1);
        tokens[0] = IICS20TransferMsgs.Token({ denom: receivedDenom, amount: transferAmount });

        IICS20TransferMsgs.FungibleTokenPacketDataV2 memory receivePacketData = IICS20TransferMsgs
            .FungibleTokenPacketDataV2({
            tokens: tokens,
            sender: senderStr,
            receiver: receiverStr,
            memo: "backmemo",
            forwarding: IICS20TransferMsgs.ForwardingPacketData({
                destinationMemo: "",
                hops: new IICS20TransferMsgs.Hop[](0)
            })
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
            sourceClient: counterpartyId,
            destClient: clientIdentifier,
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
        bytes32 storedAck = ics26Router.getCommitment(
            ICS24Host.packetAcknowledgementCommitmentKeyCalldata(packet.destClient, packet.sequence)
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
        bytes32 path = ICS24Host.packetCommitmentKeyCalldata(packet.sourceClient, packet.sequence);
        bytes32 storedCommitment = ics26Router.getCommitment(path);
        assertEq(storedCommitment, 0);

        uint256 senderBalanceAfterSend = erc20.balanceOf(sender);
        uint256 contractBalanceAfterSend = erc20.balanceOf(ics20Transfer.escrow());
        assertEq(senderBalanceAfterSend, 0);
        assertEq(contractBalanceAfterSend, transferAmount);

        // Return the tokens (receive)
        receiverStr = senderStr;
        receiver = sender;
        senderStr = "cosmos1mhmwgrfrcrdex5gnr0vcqt90wknunsxej63feh";
        IICS20TransferMsgs.Denom memory receivedDenom =
            IICS20TransferMsgs.Denom({ base: erc20AddressStr, trace: new IICS20TransferMsgs.Hop[](1) });
        receivedDenom.trace[0] = IICS20TransferMsgs.Hop({ portId: ICS20Lib.DEFAULT_PORT_ID, clientId: counterpartyId });
        IICS20TransferMsgs.Token[] memory tokens = new IICS20TransferMsgs.Token[](1);
        tokens[0] = IICS20TransferMsgs.Token({ denom: receivedDenom, amount: transferAmount });

        IICS20TransferMsgs.FungibleTokenPacketDataV2 memory receivePacketData = IICS20TransferMsgs
            .FungibleTokenPacketDataV2({
            tokens: tokens,
            sender: senderStr,
            receiver: receiverStr,
            memo: "backmemo",
            forwarding: IICS20TransferMsgs.ForwardingPacketData({
                destinationMemo: "",
                hops: new IICS20TransferMsgs.Hop[](0)
            })
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
            sourceClient: counterpartyId,
            destClient: clientIdentifier,
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
        bytes32 storedAck = ics26Router.getCommitment(
            ICS24Host.packetAcknowledgementCommitmentKeyCalldata(packet.destClient, packet.sequence)
        );
        assertEq(storedAck, ICS24Host.packetAcknowledgementCommitmentBytes32(singleSuccessAck));

        // call recv again, should be noop
        vm.expectEmit();
        emit IICS26Router.Noop();
        ics26Router.recvPacket(msgRecvPacket);
    }

    function test_success_receiveICS20PacketWithForeignBaseDenom() public {
        string memory foreignDenom = "uatom";

        senderStr = "cosmos1mhmwgrfrcrdex5gnr0vcqt90wknunsxej63feh";
        receiver = makeAddr("receiver_of_foreign_denom");
        receiverStr = Strings.toHexString(receiver);

        IICS20TransferMsgs.Token[] memory tokens = new IICS20TransferMsgs.Token[](1);
        tokens[0] = IICS20TransferMsgs.Token({
            denom: IICS20TransferMsgs.Denom({ base: foreignDenom, trace: new IICS20TransferMsgs.Hop[](0) }),
            amount: transferAmount
        });

        IICS20TransferMsgs.FungibleTokenPacketDataV2 memory receivePacketData = IICS20TransferMsgs
            .FungibleTokenPacketDataV2({
            tokens: tokens,
            sender: senderStr,
            receiver: receiverStr,
            memo: "memo",
            forwarding: IICS20TransferMsgs.ForwardingPacketData({
                destinationMemo: "",
                hops: new IICS20TransferMsgs.Hop[](0)
            })
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
            sourceClient: counterpartyId,
            destClient: clientIdentifier,
            timeoutTimestamp: uint64(block.timestamp + 1000),
            payloads: payloads
        });

        IICS20TransferMsgs.Denom memory expectedDenom =
            IICS20TransferMsgs.Denom({ base: foreignDenom, trace: new IICS20TransferMsgs.Hop[](1) });
        expectedDenom.trace[0] =
            IICS20TransferMsgs.Hop({ portId: receivePacket.payloads[0].destPort, clientId: receivePacket.destClient });

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
            ICS24Host.packetAcknowledgementCommitmentKeyCalldata(receivePacket.destClient, receivePacket.sequence)
        );
        assertEq(storedAck, ICS24Host.packetAcknowledgementCommitmentBytes32(singleSuccessAck));

        IBCERC20 ibcERC20 = IBCERC20(ics20Transfer.ibcERC20Contract(expectedDenom));
        assertEq(ibcERC20.fullDenom().base, foreignDenom);
        assertEq(ibcERC20.fullDenom().trace.length, 1);
        assertEq(ibcERC20.fullDenom().trace[0].portId, receivePacket.payloads[0].destPort);
        assertEq(ibcERC20.fullDenom().trace[0].clientId, receivePacket.destClient);
        assertEq(ibcERC20.totalSupply(), transferAmount);
        assertEq(ibcERC20.balanceOf(receiver), transferAmount);

        // Send out again
        sender = receiver;
        {
            string memory tmpSenderStr = senderStr;
            senderStr = receiverStr;
            receiverStr = tmpSenderStr;
        }

        vm.prank(sender);
        ibcERC20.approve(address(ics20Transfer), transferAmount);

        IICS20TransferMsgs.Token[] memory outboundTokens = new IICS20TransferMsgs.Token[](1);
        outboundTokens[0] = IICS20TransferMsgs.Token({ denom: expectedDenom, amount: transferAmount });
        IICS20TransferMsgs.ERC20Token[] memory outboundSendTransferMsgTokens = new IICS20TransferMsgs.ERC20Token[](1);
        outboundSendTransferMsgTokens[0] =
            IICS20TransferMsgs.ERC20Token({ contractAddress: address(ibcERC20), amount: transferAmount });

        IICS20TransferMsgs.SendTransferMsg memory msgSendTransfer = IICS20TransferMsgs.SendTransferMsg({
            tokens: outboundSendTransferMsgTokens,
            receiver: receiverStr,
            sourceClient: clientIdentifier,
            destPort: ICS20Lib.DEFAULT_PORT_ID,
            timeoutTimestamp: uint64(block.timestamp + 1000),
            memo: "backmemo",
            forwarding: IICS20TransferMsgs.Forwarding({ hops: new IICS20TransferMsgs.Hop[](0) })
        });

        IICS20TransferMsgs.FungibleTokenPacketDataV2 memory sendPacketData = IICS20TransferMsgs
            .FungibleTokenPacketDataV2({
            tokens: outboundTokens,
            sender: senderStr,
            receiver: receiverStr,
            memo: "backmemo",
            forwarding: IICS20TransferMsgs.ForwardingPacketData({
                destinationMemo: "",
                hops: new IICS20TransferMsgs.Hop[](0)
            })
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
            sourceClient: clientIdentifier,
            destClient: counterpartyId,
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
            ICS24Host.packetCommitmentKeyCalldata(expectedPacketSent.sourceClient, expectedPacketSent.sequence);
        bytes32 storedCommitment = ics26Router.getCommitment(path);
        assertEq(storedCommitment, ICS24Host.packetCommitmentBytes32(expectedPacketSent));
    }

    function test_success_receiveMultiPacketWithForeignBaseDenom() public {
        string memory foreignDenom = "uatom";

        senderStr = "cosmos1mhmwgrfrcrdex5gnr0vcqt90wknunsxej63feh";
        receiver = makeAddr("receiver_of_foreign_denom");
        receiverStr = Strings.toHexString(receiver);

        IICS20TransferMsgs.Token[] memory tokens = new IICS20TransferMsgs.Token[](1);
        tokens[0] = IICS20TransferMsgs.Token({
            denom: IICS20TransferMsgs.Denom({ base: foreignDenom, trace: new IICS20TransferMsgs.Hop[](0) }),
            amount: transferAmount
        });

        // First packet
        IICS20TransferMsgs.FungibleTokenPacketDataV2 memory packetData = IICS20TransferMsgs.FungibleTokenPacketDataV2({
            tokens: tokens,
            sender: senderStr,
            receiver: receiverStr,
            memo: "memo",
            forwarding: IICS20TransferMsgs.ForwardingPacketData({
                destinationMemo: "",
                hops: new IICS20TransferMsgs.Hop[](0)
            })
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
            sourceClient: counterpartyId,
            destClient: clientIdentifier,
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
            sourceClient: counterpartyId,
            destClient: clientIdentifier,
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
        bytes32 storedAck = ics26Router.getCommitment(
            ICS24Host.packetAcknowledgementCommitmentKeyCalldata(receivePacket.destClient, receivePacket.sequence)
        );
        assertEq(storedAck, ICS24Host.packetAcknowledgementCommitmentBytes32(singleSuccessAck));
        bytes32 storedAck2 = ics26Router.getCommitment(
            ICS24Host.packetAcknowledgementCommitmentKeyCalldata(receivePacket2.destClient, receivePacket2.sequence)
        );
        assertEq(storedAck2, ICS24Host.packetAcknowledgementCommitmentBytes32(singleSuccessAck));
    }

    function test_failure_receiveMultiPacketWithForeignBaseDenom() public {
        string memory foreignDenom = "uatom";

        senderStr = "cosmos1mhmwgrfrcrdex5gnr0vcqt90wknunsxej63feh";
        receiver = makeAddr("receiver_of_foreign_denom");
        receiverStr = Strings.toHexString(receiver);

        IICS20TransferMsgs.Token[] memory tokens = new IICS20TransferMsgs.Token[](1);
        tokens[0] = IICS20TransferMsgs.Token({
            denom: IICS20TransferMsgs.Denom({ base: foreignDenom, trace: new IICS20TransferMsgs.Hop[](0) }),
            amount: transferAmount
        });

        // First packet
        IICS20TransferMsgs.FungibleTokenPacketDataV2 memory packetData = IICS20TransferMsgs.FungibleTokenPacketDataV2({
            tokens: tokens,
            sender: senderStr,
            receiver: receiverStr,
            memo: "memo",
            forwarding: IICS20TransferMsgs.ForwardingPacketData({
                destinationMemo: "",
                hops: new IICS20TransferMsgs.Hop[](0)
            })
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
            sourceClient: counterpartyId,
            destClient: clientIdentifier,
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
            sourceClient: counterpartyId,
            destClient: clientIdentifier,
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
        senderStr = "cosmos1mhmwgrfrcrdex5gnr0vcqt90wknunsxej63feh";
        receiver = makeAddr("receiver_of_foreign_denom");
        receiverStr = Strings.toHexString(receiver);

        IICS20TransferMsgs.Token[] memory tokens = new IICS20TransferMsgs.Token[](1);
        tokens[0] = IICS20TransferMsgs.Token({
            denom: IICS20TransferMsgs.Denom({ base: "uatom", trace: new IICS20TransferMsgs.Hop[](1) }),
            amount: transferAmount
        });
        tokens[0].denom.trace[0] = IICS20TransferMsgs.Hop({ portId: ICS20Lib.DEFAULT_PORT_ID, clientId: "channel-42" });

        IICS20TransferMsgs.FungibleTokenPacketDataV2 memory receivePacketData = IICS20TransferMsgs
            .FungibleTokenPacketDataV2({
            tokens: tokens,
            sender: senderStr,
            receiver: receiverStr,
            memo: "memo",
            forwarding: IICS20TransferMsgs.ForwardingPacketData({
                destinationMemo: "",
                hops: new IICS20TransferMsgs.Hop[](0)
            })
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
            sourceClient: counterpartyId,
            destClient: clientIdentifier,
            timeoutTimestamp: uint64(block.timestamp + 1000),
            payloads: receievePayloads
        });

        IICS20TransferMsgs.Denom memory expectedDenom =
            IICS20TransferMsgs.Denom({ base: tokens[0].denom.base, trace: new IICS20TransferMsgs.Hop[](2) });
        expectedDenom.trace[0] =
            IICS20TransferMsgs.Hop({ portId: receivePacket.payloads[0].destPort, clientId: receivePacket.destClient });
        expectedDenom.trace[1] = IICS20TransferMsgs.Hop({
            portId: tokens[0].denom.trace[0].portId,
            clientId: tokens[0].denom.trace[0].clientId
        });
        string memory expectedPath = ICS20Lib.getPath(expectedDenom);
        assertEq(expectedPath, "transfer/07-tendermint-0/transfer/channel-42/uatom");

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
        bytes32 storedAck = ics26Router.getCommitment(
            ICS24Host.packetAcknowledgementCommitmentKeyCalldata(receivePacket.destClient, receivePacket.sequence)
        );
        assertEq(storedAck, ICS24Host.packetAcknowledgementCommitmentBytes32(singleSuccessAck));

        address erc20Address = address(ics20Transfer.ibcERC20Contract(expectedDenom));

        IBCERC20 ibcERC20 = IBCERC20(erc20Address);
        assertEq(ibcERC20.fullDenom().base, expectedDenom.base);
        assertEq(ibcERC20.fullDenom().trace.length, 2);
        assertEq(ibcERC20.fullDenom().trace[0].portId, expectedDenom.trace[0].portId);
        assertEq(ibcERC20.fullDenom().trace[0].clientId, expectedDenom.trace[0].clientId);
        assertEq(ibcERC20.fullDenom().trace[1].portId, expectedDenom.trace[1].portId);
        assertEq(ibcERC20.fullDenom().trace[1].clientId, expectedDenom.trace[1].clientId);
        assertEq(ibcERC20.name(), expectedPath);
        assertEq(ibcERC20.symbol(), expectedDenom.base);
        assertEq(ibcERC20.totalSupply(), transferAmount);
        assertEq(ibcERC20.balanceOf(receiver), transferAmount);

        // Send out again
        sender = receiver;
        {
            string memory tmpSenderStr = senderStr;
            senderStr = receiverStr;
            receiverStr = tmpSenderStr;
        }

        vm.prank(sender);
        ibcERC20.approve(address(ics20Transfer), transferAmount);

        IICS20TransferMsgs.Token[] memory outboundTokens = new IICS20TransferMsgs.Token[](1);
        outboundTokens[0] = IICS20TransferMsgs.Token({ denom: expectedDenom, amount: transferAmount });
        IICS20TransferMsgs.ERC20Token[] memory outboundSendTransferMsgTokens = new IICS20TransferMsgs.ERC20Token[](1);
        outboundSendTransferMsgTokens[0] =
            IICS20TransferMsgs.ERC20Token({ contractAddress: address(ibcERC20), amount: transferAmount });

        IICS20TransferMsgs.SendTransferMsg memory msgSendTransfer = IICS20TransferMsgs.SendTransferMsg({
            tokens: outboundSendTransferMsgTokens,
            receiver: receiverStr,
            sourceClient: clientIdentifier,
            destPort: ICS20Lib.DEFAULT_PORT_ID,
            timeoutTimestamp: uint64(block.timestamp + 1000),
            memo: "backmemo",
            forwarding: IICS20TransferMsgs.Forwarding({ hops: new IICS20TransferMsgs.Hop[](0) })
        });

        vm.expectEmit();

        IICS20TransferMsgs.FungibleTokenPacketDataV2 memory sendPacketData = IICS20TransferMsgs
            .FungibleTokenPacketDataV2({
            tokens: outboundTokens,
            sender: senderStr,
            receiver: receiverStr,
            memo: "backmemo",
            forwarding: IICS20TransferMsgs.ForwardingPacketData({
                destinationMemo: "",
                hops: new IICS20TransferMsgs.Hop[](0)
            })
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
            sourceClient: clientIdentifier,
            destClient: counterpartyId,
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
            ICS24Host.packetCommitmentKeyCalldata(expectedPacketSent.sourceClient, expectedPacketSent.sequence);
        bytes32 storedCommitment = ics26Router.getCommitment(path);
        assertEq(storedCommitment, ICS24Host.packetCommitmentBytes32(expectedPacketSent));
    }

    function test_success_receiveICS20PacketWithLargeAmountAndForeignIBCDenom() public {
        string memory foreignDenom = "transfer/channel-42/uatom";

        senderStr = "cosmos1mhmwgrfrcrdex5gnr0vcqt90wknunsxej63feh";
        receiver = makeAddr("receiver_of_foreign_denom");
        receiverStr = Strings.toHexString(receiver);

        uint256 largeAmount = 1_000_000_000_000_000_001;

        IICS20TransferMsgs.Token[] memory tokens = new IICS20TransferMsgs.Token[](1);
        tokens[0] = IICS20TransferMsgs.Token({
            denom: IICS20TransferMsgs.Denom({ base: foreignDenom, trace: new IICS20TransferMsgs.Hop[](0) }),
            amount: largeAmount
        });

        IICS20TransferMsgs.FungibleTokenPacketDataV2 memory receivePacketData = IICS20TransferMsgs
            .FungibleTokenPacketDataV2({
            tokens: tokens,
            sender: senderStr,
            receiver: receiverStr,
            memo: "",
            forwarding: IICS20TransferMsgs.ForwardingPacketData({
                destinationMemo: "",
                hops: new IICS20TransferMsgs.Hop[](0)
            })
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
            sourceClient: counterpartyId,
            destClient: clientIdentifier,
            timeoutTimestamp: uint64(block.timestamp + 1000),
            payloads: payloads
        });

        IICS20TransferMsgs.Denom memory expectedDenom =
            IICS20TransferMsgs.Denom({ base: foreignDenom, trace: new IICS20TransferMsgs.Hop[](1) });
        expectedDenom.trace[0] =
            IICS20TransferMsgs.Hop({ portId: receivePacket.payloads[0].destPort, clientId: receivePacket.destClient });

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
        bytes32 storedAck = ics26Router.getCommitment(
            ICS24Host.packetAcknowledgementCommitmentKeyCalldata(receivePacket.destClient, receivePacket.sequence)
        );
        assertEq(storedAck, ICS24Host.packetAcknowledgementCommitmentBytes32(singleSuccessAck));

        IBCERC20 ibcERC20 = IBCERC20(ics20Transfer.ibcERC20Contract(expectedDenom));
        assertEq(ibcERC20.totalSupply(), largeAmount);
        assertEq(ibcERC20.balanceOf(receiver), largeAmount);

        // Send out again
        sender = receiver;
        {
            string memory tmpSenderStr = senderStr;
            senderStr = receiverStr;
            receiverStr = tmpSenderStr;
        }

        vm.prank(sender);
        ibcERC20.approve(address(ics20Transfer), largeAmount);

        IICS20TransferMsgs.Token[] memory outboundTokens = new IICS20TransferMsgs.Token[](1);
        outboundTokens[0] = IICS20TransferMsgs.Token({ denom: expectedDenom, amount: largeAmount });
        IICS20TransferMsgs.ERC20Token[] memory outboundSendTransferMsgTokens = new IICS20TransferMsgs.ERC20Token[](1);
        outboundSendTransferMsgTokens[0] =
            IICS20TransferMsgs.ERC20Token({ contractAddress: address(ibcERC20), amount: largeAmount });

        IICS20TransferMsgs.SendTransferMsg memory msgSendTransfer = IICS20TransferMsgs.SendTransferMsg({
            tokens: outboundSendTransferMsgTokens,
            receiver: receiverStr,
            sourceClient: clientIdentifier,
            destPort: ICS20Lib.DEFAULT_PORT_ID,
            timeoutTimestamp: uint64(block.timestamp + 1000),
            memo: "",
            forwarding: IICS20TransferMsgs.Forwarding({ hops: new IICS20TransferMsgs.Hop[](0) })
        });

        vm.expectEmit();

        IICS20TransferMsgs.FungibleTokenPacketDataV2 memory sendPacketData = IICS20TransferMsgs
            .FungibleTokenPacketDataV2({
            tokens: outboundTokens,
            sender: senderStr,
            receiver: receiverStr,
            memo: "",
            forwarding: IICS20TransferMsgs.ForwardingPacketData({
                destinationMemo: "",
                hops: new IICS20TransferMsgs.Hop[](0)
            })
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
            sourceClient: clientIdentifier,
            destClient: counterpartyId,
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
            ICS24Host.packetCommitmentKeyCalldata(expectedPacketSent.sourceClient, expectedPacketSent.sequence);
        bytes32 storedCommitment = ics26Router.getCommitment(path);
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
        bytes32 path = ICS24Host.packetCommitmentKeyCalldata(packet.sourceClient, packet.sequence);
        bytes32 storedCommitment = ics26Router.getCommitment(path);
        assertEq(storedCommitment, 0);

        uint256 senderBalanceAfterSend = erc20.balanceOf(sender);
        uint256 contractBalanceAfterSend = erc20.balanceOf(ics20Transfer.escrow());
        assertEq(senderBalanceAfterSend, 0);
        assertEq(contractBalanceAfterSend, transferAmount);

        // Send back
        receiverStr = senderStr;
        senderStr = "cosmos1mhmwgrfrcrdex5gnr0vcqt90wknunsxej63feh";

        IICS20TransferMsgs.Denom memory denom =
            IICS20TransferMsgs.Denom({ base: erc20AddressStr, trace: new IICS20TransferMsgs.Hop[](1) });
        denom.trace[0] = IICS20TransferMsgs.Hop({ portId: counterpartyId, clientId: clientIdentifier });
        IICS20TransferMsgs.Token[] memory tokens = new IICS20TransferMsgs.Token[](1);
        tokens[0] = IICS20TransferMsgs.Token({ denom: denom, amount: transferAmount });

        IICS20TransferMsgs.FungibleTokenPacketDataV2 memory receivePacketData = IICS20TransferMsgs
            .FungibleTokenPacketDataV2({
            tokens: tokens,
            sender: senderStr,
            receiver: receiverStr,
            memo: "backmemo",
            forwarding: IICS20TransferMsgs.ForwardingPacketData({
                destinationMemo: "",
                hops: new IICS20TransferMsgs.Hop[](0)
            })
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
            sourceClient: counterpartyId,
            destClient: clientIdentifier,
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

        IICS20TransferMsgs.FungibleTokenPacketDataV2 memory defaultPacketData = _getDefaultPacketData();
        IICS20TransferMsgs.ERC20Token[] memory defaultSendTransferMsgTokens = _getDefaultSendTransferMsgTokens();

        uint256 senderBalanceBefore = erc20.balanceOf(sender);
        uint256 contractBalanceBefore = erc20.balanceOf(ics20Transfer.escrow());
        assertEq(senderBalanceBefore, transferAmount);
        assertEq(contractBalanceBefore, 0);

        uint64 timeoutTimestamp = uint64(block.timestamp + 1000);
        IICS26RouterMsgs.MsgSendPacket memory msgSendPacket = ICS20Lib.newMsgSendPacketV2(
            sender,
            IICS20TransferMsgs.SendTransferMsg({
                tokens: defaultSendTransferMsgTokens,
                receiver: defaultPacketData.receiver,
                sourceClient: clientIdentifier,
                destPort: ICS20Lib.DEFAULT_PORT_ID,
                timeoutTimestamp: timeoutTimestamp,
                memo: "memo",
                forwarding: IICS20TransferMsgs.Forwarding({ hops: new IICS20TransferMsgs.Hop[](0) })
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
            value: abi.encode(defaultPacketData)
        });
        IICS26RouterMsgs.Packet memory packet = IICS26RouterMsgs.Packet({
            sequence: sequence,
            sourceClient: msgSendPacket.sourceClient,
            destClient: counterpartyId,
            timeoutTimestamp: timeoutTimestamp,
            payloads: packetPayloads
        });

        bytes32 path = ICS24Host.packetCommitmentKeyCalldata(msgSendPacket.sourceClient, sequence);
        bytes32 storedCommitment = ics26Router.getCommitment(path);
        assertEq(storedCommitment, ICS24Host.packetCommitmentBytes32(packet));

        return packet;
    }

    function _getDefaultPacketData() internal view returns (IICS20TransferMsgs.FungibleTokenPacketDataV2 memory) {
        IICS20TransferMsgs.Token[] memory tokens = new IICS20TransferMsgs.Token[](1);
        tokens[0] = IICS20TransferMsgs.Token({
            denom: IICS20TransferMsgs.Denom({ base: erc20AddressStr, trace: new IICS20TransferMsgs.Hop[](0) }),
            amount: transferAmount
        });
        return IICS20TransferMsgs.FungibleTokenPacketDataV2({
            tokens: tokens,
            sender: senderStr,
            receiver: receiverStr,
            memo: "memo",
            forwarding: IICS20TransferMsgs.ForwardingPacketData({
                destinationMemo: "",
                hops: new IICS20TransferMsgs.Hop[](0)
            })
        });
    }

    function _getDefaultSendTransferMsgTokens() internal view returns (IICS20TransferMsgs.ERC20Token[] memory) {
        IICS20TransferMsgs.ERC20Token[] memory tokens = new IICS20TransferMsgs.ERC20Token[](1);
        tokens[0] = IICS20TransferMsgs.ERC20Token({ contractAddress: address(erc20), amount: transferAmount });
        return tokens;
    }
}
