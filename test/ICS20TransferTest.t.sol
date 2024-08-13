// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.25 <0.9.0;

// solhint-disable custom-errors,max-line-length

import { Test } from "forge-std/Test.sol";
import { IICS26RouterMsgs } from "../src/msgs/IICS26RouterMsgs.sol";
import { IIBCAppCallbacks } from "../src/msgs/IIBCAppCallbacks.sol";
import { IICS20Transfer } from "../src/interfaces/IICS20Transfer.sol";
import { ICS20Transfer } from "../src/ICS20Transfer.sol";
import { TestERC20, MalfunctioningERC20 } from "./TestERC20.sol";
import { IERC20 } from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
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
    string public receiver = "receiver";

    uint256 public defaultAmount = 1_000_000_100_000_000_001; // To account for a clear remainder
    uint256 public expectedRemainder = 100_000_000_001;
    uint256 public expectedConvertedAmount = 1_000_000_000_000_000_000; // the uint256 representation of the uint64
        // sdkCoin amount
    uint256 public defaultSdkCoinAmount = 1_000_000;

    bytes public data;
    IICS26RouterMsgs.Packet public packet;
    ICS20Lib.UnwrappedPacketData public expectedDefaultSendPacketData;

    function setUp() public {
        ics20Transfer = new ICS20Transfer(address(this));
        erc20 = new TestERC20();
        //metadataErc20 = new TestERC20Metadata(6);

        sender = makeAddr("sender");

        erc20AddressStr = Strings.toHexString(address(erc20));
        senderStr = Strings.toHexString(sender);
        data = ICS20Lib.marshalJSON(erc20AddressStr, defaultAmount, senderStr, receiver, "memo");

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

        expectedDefaultSendPacketData = ICS20Lib.UnwrappedPacketData({
            denom: erc20AddressStr,
            originatorChainIsSource: true,
            erc20Contract: address(erc20),
            sender: senderStr,
            receiver: receiver,
            amount: defaultAmount,
            memo: "memo"
        });
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
        emit IICS20Transfer.ICS20Transfer(expectedDefaultSendPacketData);
        ics20Transfer.onSendPacket(IIBCAppCallbacks.OnSendPacketCallback({ packet: packet, sender: sender }));

        uint256 senderBalanceAfter = erc20.balanceOf(sender);
        uint256 contractBalanceAfter = erc20.balanceOf(address(ics20Transfer));
        assertEq(senderBalanceAfter, expectedRemainder);
        assertEq(contractBalanceAfter, expectedConvertedAmount);
    }

    function test_failure_onSendPacket() public {
        // test missing approval
        vm.expectRevert(
            abi.encodeWithSelector(
                IERC20Errors.ERC20InsufficientAllowance.selector, address(ics20Transfer), 0, expectedConvertedAmount
            )
        );
        ics20Transfer.onSendPacket(IIBCAppCallbacks.OnSendPacketCallback({ packet: packet, sender: sender }));

        // test insufficient balance
        vm.prank(sender);
        erc20.approve(address(ics20Transfer), defaultAmount);
        vm.expectRevert(
            abi.encodeWithSelector(IERC20Errors.ERC20InsufficientBalance.selector, sender, 0, expectedConvertedAmount)
        );
        ics20Transfer.onSendPacket(IIBCAppCallbacks.OnSendPacketCallback({ packet: packet, sender: sender }));

        // test invalid amount
        data = ICS20Lib.marshalJSON(erc20AddressStr, 0, senderStr, receiver, "memo");
        packet.data = data;
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20InvalidAmount.selector, 0));
        ics20Transfer.onSendPacket(IIBCAppCallbacks.OnSendPacketCallback({ packet: packet, sender: sender }));

        // test invalid data
        data = bytes("invalid");
        packet.data = data;
        vm.expectRevert(bytes(""));
        ics20Transfer.onSendPacket(IIBCAppCallbacks.OnSendPacketCallback({ packet: packet, sender: sender }));

        // test invalid sender
        data = ICS20Lib.marshalJSON(erc20AddressStr, defaultAmount, "invalid", receiver, "memo");
        packet.data = data;
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20InvalidAddress.selector, "invalid"));
        ics20Transfer.onSendPacket(IIBCAppCallbacks.OnSendPacketCallback({ packet: packet, sender: sender }));

        // test msg sender is not packet sender
        data = ICS20Lib.marshalJSON(erc20AddressStr, defaultAmount, senderStr, receiver, "memo");
        packet.data = data;
        address someoneElse = makeAddr("someoneElse");
        vm.expectRevert(
            abi.encodeWithSelector(IICS20Errors.ICS20MsgSenderIsNotPacketSender.selector, someoneElse, sender)
        );
        ics20Transfer.onSendPacket(IIBCAppCallbacks.OnSendPacketCallback({ packet: packet, sender: someoneElse }));

        // test invalid token contract
        data = ICS20Lib.marshalJSON("invalid", defaultAmount, senderStr, receiver, "memo");
        packet.data = data;
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20InvalidTokenContract.selector, "invalid"));
        ics20Transfer.onSendPacket(IIBCAppCallbacks.OnSendPacketCallback({ packet: packet, sender: sender }));

        // test invalid version
        packet.version = "invalid";
        vm.expectRevert(
            abi.encodeWithSelector(IICS20Errors.ICS20UnexpectedVersion.selector, ICS20Lib.ICS20_VERSION, "invalid")
        );
        ics20Transfer.onSendPacket(IIBCAppCallbacks.OnSendPacketCallback({ packet: packet, sender: sender }));
        // Reset version
        packet.version = ICS20Lib.ICS20_VERSION;

        // test malfunctioning transfer
        MalfunctioningERC20 malfunctioningERC20 = new MalfunctioningERC20();
        malfunctioningERC20.mint(sender, defaultAmount);
        vm.prank(sender);
        malfunctioningERC20.approve(address(ics20Transfer), defaultAmount);
        string memory malfuncERC20AddressStr = Strings.toHexString(address(malfunctioningERC20));
        data = ICS20Lib.marshalJSON(malfuncERC20AddressStr, defaultAmount, senderStr, receiver, "memo");
        packet.data = data;
        vm.expectRevert(
            abi.encodeWithSelector(IICS20Errors.ICS20UnexpectedERC20Balance.selector, expectedConvertedAmount, 0)
        );
        ics20Transfer.onSendPacket(IIBCAppCallbacks.OnSendPacketCallback({ packet: packet, sender: sender }));
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
        emit IICS20Transfer.ICS20Transfer(expectedDefaultSendPacketData);
        ics20Transfer.onSendPacket(IIBCAppCallbacks.OnSendPacketCallback({ packet: packet, sender: sender }));

        uint256 senderBalanceAfterSend = erc20.balanceOf(sender);
        uint256 contractBalanceAfterSend = erc20.balanceOf(address(ics20Transfer));
        assertEq(senderBalanceAfterSend, expectedRemainder);
        assertEq(contractBalanceAfterSend, expectedConvertedAmount);

        vm.expectEmit();
        emit IICS20Transfer.ICS20Acknowledgement(
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
        assertEq(senderBalanceAfterAck, expectedRemainder);
        assertEq(contractBalanceAfterAck, expectedConvertedAmount);
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
        emit IICS20Transfer.ICS20Transfer(expectedDefaultSendPacketData);
        ics20Transfer.onSendPacket(IIBCAppCallbacks.OnSendPacketCallback({ packet: packet, sender: sender }));

        uint256 senderBalanceAfterSend = erc20.balanceOf(sender);
        uint256 contractBalanceAfterSend = erc20.balanceOf(address(ics20Transfer));
        assertEq(senderBalanceAfterSend, expectedRemainder);
        assertEq(contractBalanceAfterSend, expectedConvertedAmount);

        vm.expectEmit();
        emit IICS20Transfer.ICS20Acknowledgement(expectedDefaultSendPacketData, ICS20Lib.FAILED_ACKNOWLEDGEMENT_JSON);
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
        data = ICS20Lib.marshalJSON("invalid", defaultAmount, senderStr, receiver, "memo");
        packet.data = data;
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20InvalidTokenContract.selector, "invalid"));
        ics20Transfer.onAcknowledgementPacket(
            IIBCAppCallbacks.OnAcknowledgementPacketCallback({
                packet: packet,
                acknowledgement: ICS20Lib.FAILED_ACKNOWLEDGEMENT_JSON,
                relayer: makeAddr("relayer")
            })
        );

        // test invalid sender
        data = ICS20Lib.marshalJSON(erc20AddressStr, defaultAmount, "invalid", receiver, "memo");
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
        emit IICS20Transfer.ICS20Transfer(expectedDefaultSendPacketData);
        ics20Transfer.onSendPacket(IIBCAppCallbacks.OnSendPacketCallback({ packet: packet, sender: sender }));

        uint256 senderBalanceAfterSend = erc20.balanceOf(sender);
        uint256 contractBalanceAfterSend = erc20.balanceOf(address(ics20Transfer));
        assertEq(senderBalanceAfterSend, expectedRemainder);
        assertEq(contractBalanceAfterSend, expectedConvertedAmount);

        vm.expectEmit();
        emit IICS20Transfer.ICS20Timeout(expectedDefaultSendPacketData);
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
        data = ICS20Lib.marshalJSON("invalid", defaultAmount, senderStr, receiver, "memo");
        packet.data = data;
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20InvalidTokenContract.selector, "invalid"));
        ics20Transfer.onTimeoutPacket(
            IIBCAppCallbacks.OnTimeoutPacketCallback({ packet: packet, relayer: makeAddr("relayer") })
        );

        // test invalid sender
        data = ICS20Lib.marshalJSON(erc20AddressStr, defaultAmount, "invalid", receiver, "memo");
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
        emit IICS20Transfer.ICS20Transfer(expectedDefaultSendPacketData);
        ics20Transfer.onSendPacket(IIBCAppCallbacks.OnSendPacketCallback({ packet: packet, sender: sender }));

        uint256 senderBalanceAfterSend = erc20.balanceOf(sender);
        uint256 contractBalanceAfterSend = erc20.balanceOf(address(ics20Transfer));
        assertEq(senderBalanceAfterSend, expectedRemainder);
        assertEq(contractBalanceAfterSend, expectedConvertedAmount);

        // Send back (onRecv)
        string memory newSourcePort = packet.destPort;
        string memory newSourceChannel = packet.destChannel;
        string memory ibcDenom = string(abi.encodePacked(newSourcePort, "/", newSourceChannel, "/", erc20AddressStr));

        string memory backSenderStr = receiver;
        string memory backReceiverStr = senderStr;
        bytes memory receiveData = ICS20Lib.marshalJSON(ibcDenom, defaultSdkCoinAmount, receiver, senderStr, "memo");
        packet.data = receiveData;
        packet.destPort = packet.sourcePort;
        packet.destChannel = packet.sourceChannel;
        packet.sourcePort = newSourcePort;
        packet.sourceChannel = newSourceChannel;

        vm.expectEmit();
        emit IICS20Transfer.ICS20ReceiveTransfer(
            ICS20Lib.UnwrappedPacketData({
                denom: erc20AddressStr,
                originatorChainIsSource: false,
                erc20Contract: address(erc20),
                sender: backSenderStr,
                receiver: backReceiverStr,
                amount: defaultSdkCoinAmount, // Note that event still emit the unconverted amount
                memo: "memo"
            })
        );
        bytes memory ack = ics20Transfer.onRecvPacket(
            IIBCAppCallbacks.OnRecvPacketCallback({ packet: packet, relayer: makeAddr("relayer") })
        );
        assertEq(ack, ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON);

        // the tokens should have been transferred back again
        uint256 senderBalanceAfterReceive = erc20.balanceOf(sender);
        uint256 contractBalanceAfterReceive = erc20.balanceOf(address(ics20Transfer));
        assertEq(senderBalanceAfterReceive, defaultAmount);
        assertEq(contractBalanceAfterReceive, 0);
    }

    function test_success_onRecvPacketWithForeignDenom() public {
        string memory foreignDenom = "uatom";

        string memory senderAddrStr = "cosmos1mhmwgrfrcrdex5gnr0vcqt90wknunsxej63feh";
        address receiverAddr = makeAddr("receiver_of_foreign_denom");
        string memory receiverAddrStr = Strings.toHexString(receiverAddr);
        bytes memory receiveData =
            ICS20Lib.marshalJSON(foreignDenom, defaultAmount, senderAddrStr, receiverAddrStr, "memo");
        packet.data = receiveData;
        packet.destPort = "transfer";
        packet.destChannel = "dest-channel";
        packet.sourcePort = "transfer";
        packet.sourceChannel = "source-channel";

        string memory ibcDenom = "transfer/source-channel/uatom";

        vm.expectEmit(true, true, true, false); // Not checking data because we don't know the address yet
        emit IICS20Transfer.ICS20ReceiveTransfer(
            ICS20Lib.UnwrappedPacketData({
                denom: ibcDenom,
                originatorChainIsSource: true,
                erc20Contract: address(0), // unknown
                sender: senderAddrStr,
                receiver: receiverAddrStr,
                amount: defaultAmount,
                memo: "memo"
            })
        );
        vm.recordLogs();
        bytes memory ack = ics20Transfer.onRecvPacket(
            IIBCAppCallbacks.OnRecvPacketCallback({ packet: packet, relayer: makeAddr("relayer") })
        );
        assertEq(ack, ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON);

        // find and extract data from the ICS20ReceiveTransfer event
        Vm.Log[] memory entries = vm.getRecordedLogs();
        assertEq(entries.length, 4);
        Vm.Log memory receiveTransferLog = entries[3];
        assertEq(receiveTransferLog.topics[0], IICS20Transfer.ICS20ReceiveTransfer.selector);
        (ICS20Lib.UnwrappedPacketData memory receivePacketData) =
            abi.decode(receiveTransferLog.data, (ICS20Lib.UnwrappedPacketData));
        IERC20 ibcERC20 = IERC20(receivePacketData.erc20Contract);

        // finally, verify balances have been updated as expected
        assertEq(ibcERC20.totalSupply(), defaultAmount);
        assertEq(ibcERC20.balanceOf(receiverAddr), defaultAmount);
    }

    function test_failure_onRecvPacket() public {
        string memory ibcDenom =
            string(abi.encodePacked(packet.sourcePort, "/", packet.sourceChannel, "/", erc20AddressStr));
        packet.data = ICS20Lib.marshalJSON(ibcDenom, defaultAmount, receiver, senderStr, "memo");

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
        data = ICS20Lib.marshalJSON(ibcDenom, 0, receiver, senderStr, "memo");
        packet.data = data;
        ack = ics20Transfer.onRecvPacket(
            IIBCAppCallbacks.OnRecvPacketCallback({ packet: packet, relayer: makeAddr("relayer") })
        );
        assertEq(string(ack), "{\"error\":\"invalid amount: 0\"}");

        // test receiver chain is source, but denom is not erc20 address
        string memory invalidErc20Denom =
            string(abi.encodePacked(packet.sourcePort, "/", packet.sourceChannel, "/invalid"));
        data = ICS20Lib.marshalJSON(invalidErc20Denom, defaultAmount, receiver, senderStr, "memo");
        packet.data = data;
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20InvalidTokenContract.selector, "invalid"));
        ics20Transfer.onRecvPacket(
            IIBCAppCallbacks.OnRecvPacketCallback({ packet: packet, relayer: makeAddr("relayer") })
        );

        // test invalid receiver
        data = ICS20Lib.marshalJSON(ibcDenom, defaultAmount, receiver, "invalid", "memo");
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
            receiver,
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
