// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

// solhint-disable custom-errors,max-line-length

import { Test } from "forge-std/Test.sol";

import { IICS26RouterMsgs } from "../../contracts/msgs/IICS26RouterMsgs.sol";
import { IICS20TransferMsgs } from "../../contracts/msgs/IICS20TransferMsgs.sol";

import { IICS20Errors } from "../../contracts/errors/IICS20Errors.sol";
import { IIBCAppCallbacks } from "../../contracts/msgs/IIBCAppCallbacks.sol";
import { IERC20Errors } from "@openzeppelin-contracts/interfaces/draft-IERC6093.sol";
import { IICS26Router } from "../../contracts/interfaces/IICS26Router.sol";

import { ICS20Transfer } from "../../contracts/ICS20Transfer.sol";
import { TestERC20, MalfunctioningERC20 } from "./mocks/TestERC20.sol";
import { ICS20Lib } from "../../contracts/utils/ICS20Lib.sol";
import { ICS24Host } from "../../contracts/utils/ICS24Host.sol";
import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";
import { ERC1967Proxy } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Proxy.sol";
import { IBCERC20 } from "../../contracts/utils/IBCERC20.sol";
import { Escrow } from "../../contracts/utils/Escrow.sol";

contract ICS20TransferTest is Test {
    ICS20Transfer public ics20Transfer;
    TestERC20 public erc20;

    address public sender;
    string public senderStr;
    address public receiver;
    string public receiverStr;

    /// @dev the default send amount for sendTransfer
    uint256 public defaultAmount = 1_000_000_100_000_000_001;

    function setUp() public {
        address escrowLogic = address(new Escrow());
        address ibcERC20Logic = address(new IBCERC20());
        ICS20Transfer ics20TransferLogic = new ICS20Transfer();

        ERC1967Proxy transferProxy = new ERC1967Proxy(
            address(ics20TransferLogic),
            abi.encodeWithSelector(
                ICS20Transfer.initialize.selector, address(this), escrowLogic, ibcERC20Logic, address(0)
            )
        );

        ics20Transfer = ICS20Transfer(address(transferProxy));
        erc20 = new TestERC20();

        sender = makeAddr("sender");
        senderStr = Strings.toHexString(sender);

        receiver = makeAddr(receiverStr);
        receiverStr = Strings.toHexString(receiver);
    }

    function test_success_sendTransfer() public {
        (IICS26RouterMsgs.Packet memory packet,) = _getDefaultPacket();

        erc20.mint(sender, defaultAmount);
        vm.prank(sender);
        erc20.approve(address(ics20Transfer), defaultAmount);

        IICS20TransferMsgs.SendTransferMsg memory msgSendTransfer = IICS20TransferMsgs.SendTransferMsg({
            denom: address(erc20),
            amount: defaultAmount,
            receiver: receiverStr,
            sourceClient: packet.sourceClient,
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
        // this contract acts as the ics26Router

        // test missing approval
        vm.expectRevert(
            abi.encodeWithSelector(
                IERC20Errors.ERC20InsufficientAllowance.selector, address(ics20Transfer), 0, defaultAmount
            )
        );
        ics20Transfer.sendTransfer(_getTestSendTransferMsg());

        // test insufficient balance
        vm.prank(sender);
        erc20.approve(address(ics20Transfer), defaultAmount);
        vm.expectRevert(
            abi.encodeWithSelector(IERC20Errors.ERC20InsufficientBalance.selector, sender, 0, defaultAmount)
        );
        vm.prank(sender);
        ics20Transfer.sendTransfer(_getTestSendTransferMsg());

        erc20.mint(sender, defaultAmount);

        // test invalid amount
        defaultAmount = 0;
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20InvalidAmount.selector, 0));
        vm.prank(sender);
        ics20Transfer.sendTransfer(_getTestSendTransferMsg());

        // reset amount
        defaultAmount = 1_000_000_100_000_000_001;

        // test malfunctioning transfer
        MalfunctioningERC20 malfunctioningERC20 = new MalfunctioningERC20();
        malfunctioningERC20.mint(sender, defaultAmount);
        malfunctioningERC20.setMalfunction(true); // Turn on the malfunctioning behaviour (no update)
        vm.prank(sender);
        malfunctioningERC20.approve(address(ics20Transfer), defaultAmount);
        erc20 = malfunctioningERC20; // This is not reset since it is the last test

        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20UnexpectedERC20Balance.selector, defaultAmount, 0));
        vm.prank(sender);
        ics20Transfer.sendTransfer(_getTestSendTransferMsg());
    }

    function test_failure_onAcknowledgementPacket() public {
        (IICS26RouterMsgs.Packet memory packet, IICS20TransferMsgs.FungibleTokenPacketData memory defaultPacketData) =
            _getDefaultPacket();

        // test invalid data
        bytes memory data = bytes("invalid");
        packet.payloads[0].value = data;
        vm.expectRevert();
        ics20Transfer.onAcknowledgementPacket(
            IIBCAppCallbacks.OnAcknowledgementPacketCallback({
                sourceClient: packet.sourceClient,
                destinationClient: packet.destClient,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                acknowledgement: ICS24Host.UNIVERSAL_ERROR_ACK,
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
                sourceClient: packet.sourceClient,
                destinationClient: packet.destClient,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                acknowledgement: abi.encodePacked(ICS24Host.UNIVERSAL_ERROR_ACK),
                relayer: makeAddr("relayer")
            })
        );
        // reset denom
        defaultPacketData.denom = Strings.toHexString(address(erc20));
        packet.payloads[0].value = abi.encode(defaultPacketData);

        // test denom not found when sending a non-native token (source trace)
        defaultPacketData.denom =
            string(abi.encodePacked(packet.payloads[0].sourcePort, "/", packet.sourceClient, "/", "notfound"));
        packet.payloads[0].value = abi.encode(defaultPacketData);
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20DenomNotFound.selector, defaultPacketData.denom));
        ics20Transfer.onAcknowledgementPacket(
            IIBCAppCallbacks.OnAcknowledgementPacketCallback({
                sourceClient: packet.sourceClient,
                destinationClient: packet.destClient,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                acknowledgement: abi.encodePacked(ICS24Host.UNIVERSAL_ERROR_ACK),
                relayer: makeAddr("relayer")
            })
        );
        // reset denom
        defaultPacketData.denom = Strings.toHexString(address(erc20));
        packet.payloads[0].value = abi.encode(defaultPacketData);

        // test invalid sender
        defaultPacketData.sender = "invalid";
        packet.payloads[0].value = abi.encode(defaultPacketData);
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20InvalidAddress.selector, "invalid"));
        ics20Transfer.onAcknowledgementPacket(
            IIBCAppCallbacks.OnAcknowledgementPacketCallback({
                sourceClient: packet.sourceClient,
                destinationClient: packet.destClient,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                acknowledgement: abi.encodePacked(ICS24Host.UNIVERSAL_ERROR_ACK),
                relayer: makeAddr("relayer")
            })
        );
        // reset sender
        defaultPacketData.sender = senderStr;
        packet.payloads[0].value = abi.encode(defaultPacketData);
    }

    function test_failure_onTimeoutPacket() public {
        (IICS26RouterMsgs.Packet memory packet, IICS20TransferMsgs.FungibleTokenPacketData memory defaultPacketData) =
            _getDefaultPacket();

        // test invalid data
        bytes memory data = bytes("invalid");
        packet.payloads[0].value = data;
        vm.expectRevert(bytes(""));
        ics20Transfer.onTimeoutPacket(
            IIBCAppCallbacks.OnTimeoutPacketCallback({
                sourceClient: packet.sourceClient,
                destinationClient: packet.destClient,
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
                sourceClient: packet.sourceClient,
                destinationClient: packet.destClient,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                relayer: makeAddr("relayer")
            })
        );
        // reset denom
        defaultPacketData.denom = Strings.toHexString(address(erc20));
        packet.payloads[0].value = abi.encode(defaultPacketData);

        // test denom not found when sending a non-native token (source trace)
        defaultPacketData.denom =
            string(abi.encodePacked(packet.payloads[0].sourcePort, "/", packet.sourceClient, "/", "notfound"));
        packet.payloads[0].value = abi.encode(defaultPacketData);
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20DenomNotFound.selector, defaultPacketData.denom));
        ics20Transfer.onTimeoutPacket(
            IIBCAppCallbacks.OnTimeoutPacketCallback({
                sourceClient: packet.sourceClient,
                destinationClient: packet.destClient,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                relayer: makeAddr("relayer")
            })
        );
        // reset denom
        defaultPacketData.denom = Strings.toHexString(address(erc20));
        packet.payloads[0].value = abi.encode(defaultPacketData);

        // test invalid sender
        defaultPacketData.sender = "invalid";
        packet.payloads[0].value = abi.encode(defaultPacketData);
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20InvalidAddress.selector, "invalid"));
        ics20Transfer.onTimeoutPacket(
            IIBCAppCallbacks.OnTimeoutPacketCallback({
                sourceClient: packet.sourceClient,
                destinationClient: packet.destClient,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                relayer: makeAddr("relayer")
            })
        );
        // reset sender
        defaultPacketData.sender = senderStr;
        packet.payloads[0].value = abi.encode(defaultPacketData);
    }

    function test_failure_onRecvPacket() public {
        (IICS26RouterMsgs.Packet memory packet, IICS20TransferMsgs.FungibleTokenPacketData memory defaultPacketData) =
            _getDefaultPacket();

        string memory ibcDenom = string(
            abi.encodePacked(
                packet.payloads[0].sourcePort, "/", packet.sourceClient, "/", Strings.toHexString(address(erc20))
            )
        );
        defaultPacketData.denom = ibcDenom;
        packet.payloads[0].value = abi.encode(defaultPacketData);

        // test invalid version
        packet.payloads[0].version = "invalid";
        vm.expectRevert(
            abi.encodeWithSelector(IICS20Errors.ICS20UnexpectedVersion.selector, ICS20Lib.ICS20_VERSION, "invalid")
        );
        ics20Transfer.onRecvPacket(
            IIBCAppCallbacks.OnRecvPacketCallback({
                sourceClient: packet.sourceClient,
                destinationClient: packet.destClient,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                relayer: makeAddr("relayer")
            })
        );
        // Reset version
        packet.payloads[0].version = ICS20Lib.ICS20_VERSION;
        packet.payloads[0].value = abi.encode(defaultPacketData);

        // test invalid data
        bytes memory data = bytes("invalid");
        packet.payloads[0].value = data;
        vm.expectRevert(); // here we expect a generic revert caused by the abi.decodePayload function
        ics20Transfer.onRecvPacket(
            IIBCAppCallbacks.OnRecvPacketCallback({
                sourceClient: packet.sourceClient,
                destinationClient: packet.destClient,
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
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20InvalidAmount.selector, 0));
        ics20Transfer.onRecvPacket(
            IIBCAppCallbacks.OnRecvPacketCallback({
                sourceClient: packet.sourceClient,
                destinationClient: packet.destClient,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                relayer: makeAddr("relayer")
            })
        );
        // reset amount
        defaultPacketData.amount = defaultAmount;
        packet.payloads[0].value = abi.encode(defaultPacketData);

        // test receiver chain is source, but denom is not erc20 address
        string memory invalidErc20Denom =
            string(abi.encodePacked(packet.payloads[0].sourcePort, "/", packet.sourceClient, "/invalid"));
        defaultPacketData.denom = invalidErc20Denom;
        packet.payloads[0].value = abi.encode(defaultPacketData);
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20DenomNotFound.selector, "invalid"));
        ics20Transfer.onRecvPacket(
            IIBCAppCallbacks.OnRecvPacketCallback({
                sourceClient: packet.sourceClient,
                destinationClient: packet.destClient,
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
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20InvalidAddress.selector, "invalid"));
        ics20Transfer.onRecvPacket(
            IIBCAppCallbacks.OnRecvPacketCallback({
                sourceClient: packet.sourceClient,
                destinationClient: packet.destClient,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                relayer: makeAddr("relayer")
            })
        );
        // reset receiver
        defaultPacketData.receiver = receiverStr;
        packet.payloads[0].value = abi.encode(defaultPacketData);
    }

    function _getDefaultPacket()
        internal
        view
        returns (IICS26RouterMsgs.Packet memory, IICS20TransferMsgs.FungibleTokenPacketData memory)
    {
        IICS20TransferMsgs.FungibleTokenPacketData memory defaultPacketData = _getDefaultPacketData();
        bytes memory data = abi.encode(defaultPacketData);
        IICS26RouterMsgs.Payload[] memory payloads = new IICS26RouterMsgs.Payload[](1);
        payloads[0] = IICS26RouterMsgs.Payload({
            sourcePort: "sourcePort",
            destPort: "destinationPort",
            version: ICS20Lib.ICS20_VERSION,
            encoding: ICS20Lib.ICS20_ENCODING,
            value: data
        });
        return (
            IICS26RouterMsgs.Packet({
                sequence: 0,
                sourceClient: "sourceClient",
                destClient: "destinationClient",
                timeoutTimestamp: 0,
                payloads: payloads
            }),
            defaultPacketData
        );
    }

    function _getDefaultPacketData() internal view returns (IICS20TransferMsgs.FungibleTokenPacketData memory) {
        IICS20TransferMsgs.FungibleTokenPacketData memory defaultPacketData = IICS20TransferMsgs.FungibleTokenPacketData({
            denom: Strings.toHexString(address(erc20)),
            amount: defaultAmount,
            sender: senderStr,
            receiver: receiverStr,
            memo: "memo"
        });

        return defaultPacketData;
    }

    function _getTestSendTransferMsg() internal view returns (IICS20TransferMsgs.SendTransferMsg memory) {
        return IICS20TransferMsgs.SendTransferMsg({
            denom: address(erc20),
            amount: defaultAmount,
            receiver: receiverStr,
            sourceClient: "sourceClient",
            destPort: "destinationPort",
            timeoutTimestamp: 0,
            memo: "memo"
        });
    }
}
