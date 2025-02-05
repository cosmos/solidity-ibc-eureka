// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

// solhint-disable custom-errors,max-line-length

import { Test } from "forge-std/Test.sol";
import { IICS26RouterMsgs } from "../../contracts/msgs/IICS26RouterMsgs.sol";
import { IICS26Router } from "../../contracts/interfaces/IICS26Router.sol";
import { IIBCAppCallbacks } from "../../contracts/msgs/IIBCAppCallbacks.sol";
import { IICS20TransferMsgs } from "../../contracts/msgs/IICS20TransferMsgs.sol";
import { ICS20Transfer } from "../../contracts/ICS20Transfer.sol";
import { TestERC20, MalfunctioningERC20 } from "./mocks/TestERC20.sol";
import { IERC20Errors } from "@openzeppelin-contracts/interfaces/draft-IERC6093.sol";
import { ICS20Lib } from "../../contracts/utils/ICS20Lib.sol";
import { IICS20Errors } from "../../contracts/errors/IICS20Errors.sol";
import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";
import { TransparentUpgradeableProxy } from "@openzeppelin-contracts/proxy/transparent/TransparentUpgradeableProxy.sol";

contract ICS20TransferTest is Test {
    ICS20Transfer public ics20Transfer;
    TestERC20 public erc20;
    string public erc20AddressStr;

    address public sender;
    string public senderStr;
    address public receiver;
    string public receiverStr;

    /// @dev the default send amount for sendTransfer
    uint256 public defaultAmount = 1_000_000_100_000_000_001;

    function setUp() public {
        ICS20Transfer ics20TransferLogic = new ICS20Transfer();

        TransparentUpgradeableProxy transferProxy = new TransparentUpgradeableProxy(
            address(ics20TransferLogic),
            address(this),
            abi.encodeWithSelector(ICS20Transfer.initialize.selector, address(this), address(0))
        );

        ics20Transfer = ICS20Transfer(address(transferProxy));
        erc20 = new TestERC20();

        sender = makeAddr("sender");
        senderStr = Strings.toHexString(sender);

        receiver = makeAddr(receiverStr);
        receiverStr = Strings.toHexString(receiver);

        erc20AddressStr = Strings.toHexString(address(erc20));
    }

    function test_success_sendTransfer() public {
        (IICS26RouterMsgs.Packet memory packet,) = _getDefaultPacket();

        IICS20TransferMsgs.SendTransferMsg memory msgSendTransfer = IICS20TransferMsgs.SendTransferMsg({
            denom: erc20AddressStr,
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
        (IICS26RouterMsgs.Packet memory packet,) = _getDefaultPacket();

        // just to make sure it doesn't accidentally revert on the router call
        vm.mockCall(address(this), abi.encodeWithSelector(IICS26Router.sendPacket.selector), abi.encode(uint32(42)));

        vm.startPrank(sender);

        IICS20TransferMsgs.SendTransferMsg memory msgSendTransfer = IICS20TransferMsgs.SendTransferMsg({
            denom: erc20AddressStr,
            amount: defaultAmount,
            receiver: receiverStr,
            sourceClient: packet.sourceClient,
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

    function test_failure_onSendPacket() public {
        (IICS26RouterMsgs.Packet memory packet, ICS20Lib.FungibleTokenPacketData memory defaultPacketData) =
            _getDefaultPacket();

        // this contract acts as the ics26Router (it is the address given as owner to the ics20Transfer contract)

        // test missing approval
        vm.expectRevert(
            abi.encodeWithSelector(
                IERC20Errors.ERC20InsufficientAllowance.selector, address(ics20Transfer), 0, defaultAmount
            )
        );
        ics20Transfer.onSendPacket(
            IIBCAppCallbacks.OnSendPacketCallback({
                sourceClient: packet.sourceClient,
                destinationClient: packet.destClient,
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
                sourceClient: packet.sourceClient,
                destinationClient: packet.destClient,
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
                sourceClient: packet.sourceClient,
                destinationClient: packet.destClient,
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
                sourceClient: packet.sourceClient,
                destinationClient: packet.destClient,
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
                sourceClient: packet.sourceClient,
                destinationClient: packet.destClient,
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
                sourceClient: packet.sourceClient,
                destinationClient: packet.destClient,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                sender: notSender // not the same as the payload sender
             })
        );

        // test msg sender is sender, i.e. not owner (ics26Router)
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20Unauthorized.selector, sender));
        vm.prank(sender);
        ics20Transfer.onSendPacket(
            IIBCAppCallbacks.OnSendPacketCallback({
                sourceClient: packet.sourceClient,
                destinationClient: packet.destClient,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                sender: sender
            })
        );

        // test msg sender is someone else entierly, i.e. owner (ics26Router)
        address someoneElse = makeAddr("someoneElse");
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20Unauthorized.selector, someoneElse));
        vm.prank(someoneElse);
        ics20Transfer.onSendPacket(
            IIBCAppCallbacks.OnSendPacketCallback({
                sourceClient: packet.sourceClient,
                destinationClient: packet.destClient,
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
                sourceClient: packet.sourceClient,
                destinationClient: packet.destClient,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                sender: sender
            })
        );
        // reset denom
        defaultPacketData.denom = erc20AddressStr;
        packet.payloads[0].value = abi.encode(defaultPacketData);

        // test denom not found when sending a non-native token (source trace)
        defaultPacketData.denom =
            string(abi.encodePacked(packet.payloads[0].sourcePort, "/", packet.sourceClient, "/", "notfound"));
        packet.payloads[0].value = abi.encode(defaultPacketData);
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20DenomNotFound.selector, defaultPacketData.denom));
        ics20Transfer.onSendPacket(
            IIBCAppCallbacks.OnSendPacketCallback({
                sourceClient: packet.sourceClient,
                destinationClient: packet.destClient,
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
                sourceClient: packet.sourceClient,
                destinationClient: packet.destClient,
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
                sourceClient: packet.sourceClient,
                destinationClient: packet.destClient,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                sender: address(ics20Transfer)
            })
        );
        // reset denom
        defaultPacketData.denom = erc20AddressStr;
        packet.payloads[0].value = abi.encode(defaultPacketData);
    }

    function test_failure_onAcknowledgementPacket() public {
        (IICS26RouterMsgs.Packet memory packet, ICS20Lib.FungibleTokenPacketData memory defaultPacketData) =
            _getDefaultPacket();

        // test invalid data
        bytes memory data = bytes("invalid");
        packet.payloads[0].value = data;
        vm.expectRevert(bytes(""));
        ics20Transfer.onAcknowledgementPacket(
            IIBCAppCallbacks.OnAcknowledgementPacketCallback({
                sourceClient: packet.sourceClient,
                destinationClient: packet.destClient,
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
                sourceClient: packet.sourceClient,
                destinationClient: packet.destClient,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                acknowledgement: ICS20Lib.FAILED_ACKNOWLEDGEMENT_JSON,
                relayer: makeAddr("relayer")
            })
        );
        // reset denom
        defaultPacketData.denom = erc20AddressStr;
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
                sourceClient: packet.sourceClient,
                destinationClient: packet.destClient,
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

    function test_failure_onTimeoutPacket() public {
        (IICS26RouterMsgs.Packet memory packet, ICS20Lib.FungibleTokenPacketData memory defaultPacketData) =
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
        defaultPacketData.denom = erc20AddressStr;
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
        defaultPacketData.denom = erc20AddressStr;
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
        (IICS26RouterMsgs.Packet memory packet, ICS20Lib.FungibleTokenPacketData memory defaultPacketData) =
            _getDefaultPacket();

        string memory ibcDenom =
            string(abi.encodePacked(packet.payloads[0].sourcePort, "/", packet.sourceClient, "/", erc20AddressStr));
        defaultPacketData.denom = ibcDenom;
        packet.payloads[0].value = abi.encode(defaultPacketData);

        // test invalid version
        packet.payloads[0].version = "invalid";
        bytes memory ack = ics20Transfer.onRecvPacket(
            IIBCAppCallbacks.OnRecvPacketCallback({
                sourceClient: packet.sourceClient,
                destinationClient: packet.destClient,
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
        ack = ics20Transfer.onRecvPacket(
            IIBCAppCallbacks.OnRecvPacketCallback({
                sourceClient: packet.sourceClient,
                destinationClient: packet.destClient,
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
        ack = ics20Transfer.onRecvPacket(
            IIBCAppCallbacks.OnRecvPacketCallback({
                sourceClient: packet.sourceClient,
                destinationClient: packet.destClient,
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

    function _getDefaultPacket()
        internal
        view
        returns (IICS26RouterMsgs.Packet memory, ICS20Lib.FungibleTokenPacketData memory)
    {
        ICS20Lib.FungibleTokenPacketData memory defaultPacketData = _getDefaultPacketData();
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

    function _getDefaultPacketData() internal view returns (ICS20Lib.FungibleTokenPacketData memory) {
        ICS20Lib.FungibleTokenPacketData memory defaultPacketData = ICS20Lib.FungibleTokenPacketData({
            denom: erc20AddressStr,
            amount: defaultAmount,
            sender: senderStr,
            receiver: receiverStr,
            memo: "memo"
        });

        return defaultPacketData;
    }
}
