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
    string public receiverStr = "receiver";

    /// @dev the default send amount for sendTransfer
    uint256 public defaultAmount = 1_000_000_100_000_000_001;

    function setUp() public {
        ICS20Transfer ics20TransferLogic = new ICS20Transfer();

        TransparentUpgradeableProxy transferProxy = new TransparentUpgradeableProxy(
            address(ics20TransferLogic),
            address(this),
            abi.encodeWithSelector(ICS20Transfer.initialize.selector, address(this))
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

        IICS20TransferMsgs.ERC20Token[] memory tokens = new IICS20TransferMsgs.ERC20Token[](1);
        tokens[0] = IICS20TransferMsgs.ERC20Token({ contractAddress: address(erc20), amount: defaultAmount });
        IICS20TransferMsgs.SendTransferMsg memory msgSendTransfer = IICS20TransferMsgs.SendTransferMsg({
            tokens: tokens,
            receiver: receiverStr,
            sourceClient: packet.sourceClient,
            destPort: packet.payloads[0].sourcePort,
            timeoutTimestamp: uint64(block.timestamp + 1000),
            memo: "memo",
            forwarding: IICS20TransferMsgs.Forwarding({ hops: new IICS20TransferMsgs.Hop[](0) })
        });

        vm.mockCall(address(this), abi.encodeWithSelector(IICS26Router.sendPacket.selector), abi.encode(uint32(42)));
        vm.prank(sender);
        uint32 sequence = ics20Transfer.sendTransfer(msgSendTransfer);
        assertEq(sequence, 42);
    }

    function test_newMsgSendPacketV2() public {
        address senderAddress = makeAddr("my-sender");
        string memory expectedSenderStr = Strings.toHexString(senderAddress);
        IICS20TransferMsgs.ERC20Token[] memory erc20Tokens = new IICS20TransferMsgs.ERC20Token[](1);
        erc20Tokens[0] = IICS20TransferMsgs.ERC20Token({ contractAddress: address(erc20), amount: defaultAmount });

        IICS20TransferMsgs.SendTransferMsg memory msgSendTransfer = IICS20TransferMsgs.SendTransferMsg({
            tokens: erc20Tokens,
            receiver: "my-receiver",
            sourceClient: "my-source-client",
            destPort: "my-dest-port",
            timeoutTimestamp: 123,
            memo: "my-memo",
            forwarding: IICS20TransferMsgs.Forwarding({ hops: new IICS20TransferMsgs.Hop[](0) })
        });

        IICS20TransferMsgs.Token[] memory expectedTokens = new IICS20TransferMsgs.Token[](1);
        expectedTokens[0] = IICS20TransferMsgs.Token({
            denom: IICS20TransferMsgs.Denom({ base: erc20AddressStr, trace: new IICS20TransferMsgs.Hop[](0) }),
            amount: defaultAmount
        });
        IICS20TransferMsgs.FungibleTokenPacketDataV2 memory expectedPacketData = IICS20TransferMsgs
            .FungibleTokenPacketDataV2({
            tokens: expectedTokens,
            sender: expectedSenderStr,
            receiver: msgSendTransfer.receiver,
            memo: msgSendTransfer.memo,
            forwarding: IICS20TransferMsgs.ForwardingPacketData({
                destinationMemo: "",
                hops: new IICS20TransferMsgs.Hop[](0)
            })
        });

        IICS26RouterMsgs.MsgSendPacket memory msgSendPacket =
            ics20Transfer.newMsgSendPacketV2(senderAddress, msgSendTransfer);
        assertEq(msgSendPacket.sourceClient, msgSendTransfer.sourceClient);
        assertEq(msgSendPacket.timeoutTimestamp, msgSendTransfer.timeoutTimestamp);
        assertEq(msgSendPacket.payloads.length, 1);
        assertEq(msgSendPacket.payloads[0].sourcePort, ICS20Lib.DEFAULT_PORT_ID);
        assertEq(msgSendPacket.payloads[0].destPort, msgSendTransfer.destPort);
        assertEq(msgSendPacket.payloads[0].version, ICS20Lib.ICS20_VERSION);
        assertEq(msgSendPacket.payloads[0].encoding, ICS20Lib.ICS20_ENCODING);
        assertEq(msgSendPacket.payloads[0].value, abi.encode(expectedPacketData));
    }

    function test_failure_onSendPacket() public {
        (IICS26RouterMsgs.Packet memory packet, IICS20TransferMsgs.FungibleTokenPacketDataV2 memory defaultPacketData) =
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
        defaultPacketData.tokens[0].amount = 0;
        packet.payloads[0].value = abi.encode(defaultPacketData);
        vm.expectRevert(
            abi.encodeWithSelector(IICS20Errors.ICS20InvalidPacketData.selector, "amount must be greater than 0")
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
        // reset amount
        defaultPacketData.tokens[0].amount = defaultAmount;
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

        // test sender length is too long
        defaultPacketData.sender = generateLongString(ICS20Lib.MAX_SENDER_RECEIVER_LENGTH + 1);
        packet.payloads[0].value = abi.encode(defaultPacketData);
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20InvalidPacketData.selector, "sender too long"));
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

        // test receiver length is too long
        defaultPacketData.receiver = generateLongString(ICS20Lib.MAX_SENDER_RECEIVER_LENGTH + 1);
        packet.payloads[0].value = abi.encode(defaultPacketData);
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20InvalidPacketData.selector, "receiver too long"));
        ics20Transfer.onSendPacket(
            IIBCAppCallbacks.OnSendPacketCallback({
                sourceClient: packet.sourceClient,
                destinationClient: packet.destClient,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                sender: sender
            })
        );
        // reset receiver
        defaultPacketData.receiver = receiverStr;
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
        defaultPacketData.tokens[0].denom.base = "invalid";
        packet.payloads[0].value = abi.encode(defaultPacketData);
        vm.expectRevert(
            abi.encodeWithSelector(IICS20Errors.ICS20DenomNotFound.selector, defaultPacketData.tokens[0].denom)
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
        // reset denom
        defaultPacketData.tokens[0].denom.base = erc20AddressStr;
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

        // test memo set in packet data with forwarding also set
        defaultPacketData.forwarding.hops = new IICS20TransferMsgs.Hop[](1);
        defaultPacketData.forwarding.hops[0] = IICS20TransferMsgs.Hop({ portId: "port", clientId: "client" });
        defaultPacketData.memo = "memo";
        packet.payloads[0].value = abi.encode(defaultPacketData);
        vm.expectRevert(
            abi.encodeWithSelector(
                IICS20Errors.ICS20InvalidPacketData.selector, "memo must be empty if forwarding is set"
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
        // reset memo and forwarding
        defaultPacketData.memo = "";
        defaultPacketData.forwarding.hops = new IICS20TransferMsgs.Hop[](0);

        // test forwarding memo set with no hops
        defaultPacketData.forwarding.destinationMemo = "destination-memo";
        packet.payloads[0].value = abi.encode(defaultPacketData);
        vm.expectRevert(
            abi.encodeWithSelector(
                IICS20Errors.ICS20InvalidPacketData.selector, "destinationMemo must be empty if forwarding is not set"
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
        // reset forwarding memo
        defaultPacketData.forwarding.destinationMemo = "";

        // test memo too long
        defaultPacketData.memo = generateLongString(ICS20Lib.MAX_MEMO_LENGTH + 1);
        packet.payloads[0].value = abi.encode(defaultPacketData);
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20InvalidPacketData.selector, "memo too long"));
        ics20Transfer.onSendPacket(
            IIBCAppCallbacks.OnSendPacketCallback({
                sourceClient: packet.sourceClient,
                destinationClient: packet.destClient,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                sender: sender
            })
        );
        // reset memo
        defaultPacketData.memo = "";

        // test forwarding memo too long
        defaultPacketData.forwarding.destinationMemo = generateLongString(ICS20Lib.MAX_MEMO_LENGTH + 1);
        // Need to set hops, otherwise we would hit the destination memo must be empty if forwarding is not set error
        defaultPacketData.forwarding.hops = new IICS20TransferMsgs.Hop[](1);
        defaultPacketData.forwarding.hops[0] = IICS20TransferMsgs.Hop({ portId: "port", clientId: "client" });
        packet.payloads[0].value = abi.encode(defaultPacketData);
        vm.expectRevert(
            abi.encodeWithSelector(IICS20Errors.ICS20InvalidPacketData.selector, "destinationMemo too long")
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
        // reset forwarding memo and hops
        defaultPacketData.forwarding.destinationMemo = "";
        defaultPacketData.forwarding.hops = new IICS20TransferMsgs.Hop[](0);

        // test empty port ID in forwarding
        defaultPacketData.forwarding.hops = new IICS20TransferMsgs.Hop[](1);
        defaultPacketData.forwarding.hops[0] = IICS20TransferMsgs.Hop({ portId: "", clientId: "client" });
        packet.payloads[0].value = abi.encode(defaultPacketData);
        vm.expectRevert(
            abi.encodeWithSelector(IICS20Errors.ICS20InvalidPacketData.selector, "portId must be set for each hop")
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
        // reset forwarding
        defaultPacketData.forwarding.hops = new IICS20TransferMsgs.Hop[](0);

        // test empty client ID in forwarding
        defaultPacketData.forwarding.hops = new IICS20TransferMsgs.Hop[](1);
        defaultPacketData.forwarding.hops[0] = IICS20TransferMsgs.Hop({ portId: "port", clientId: "" });
        packet.payloads[0].value = abi.encode(defaultPacketData);
        vm.expectRevert(
            abi.encodeWithSelector(IICS20Errors.ICS20InvalidPacketData.selector, "clientId must be set for each hop")
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
        // reset forwarding
        defaultPacketData.forwarding.hops = new IICS20TransferMsgs.Hop[](0);

        // test too many hops in forwarding
        defaultPacketData.forwarding.hops = new IICS20TransferMsgs.Hop[](ICS20Lib.MAX_HOPS + 1);
        for (uint256 i = 0; i < ICS20Lib.MAX_HOPS + 1; i++) {
            defaultPacketData.forwarding.hops[i] = IICS20TransferMsgs.Hop({ portId: "port", clientId: "client" });
        }
        packet.payloads[0].value = abi.encode(defaultPacketData);
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20InvalidPacketData.selector, "too many hops"));
        ics20Transfer.onSendPacket(
            IIBCAppCallbacks.OnSendPacketCallback({
                sourceClient: packet.sourceClient,
                destinationClient: packet.destClient,
                sequence: packet.sequence,
                payload: packet.payloads[0],
                sender: sender
            })
        );
        // reset forwarding
        defaultPacketData.forwarding.hops = new IICS20TransferMsgs.Hop[](0);

        // test malfunctioning transfer
        MalfunctioningERC20 malfunctioningERC20 = new MalfunctioningERC20();
        malfunctioningERC20.mint(sender, defaultAmount);
        malfunctioningERC20.setMalfunction(true); // Turn on the malfunctioning behaviour (no update)
        vm.prank(sender);
        malfunctioningERC20.approve(address(ics20Transfer), defaultAmount);
        string memory malfuncERC20AddressStr = Strings.toHexString(address(malfunctioningERC20));

        defaultPacketData.tokens[0].denom.base = malfuncERC20AddressStr;
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
        defaultPacketData.tokens[0].denom.base = erc20AddressStr;
        packet.payloads[0].value = abi.encode(defaultPacketData);
    }

    function test_failure_onAcknowledgementPacket() public {
        (IICS26RouterMsgs.Packet memory packet, IICS20TransferMsgs.FungibleTokenPacketDataV2 memory defaultPacketData) =
            _getDefaultPacket();

        // test invalid data
        packet.payloads[0].value = bytes("invalid");
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
        defaultPacketData.tokens[0].denom.base = "invalid";
        packet.payloads[0].value = abi.encode(defaultPacketData);
        vm.expectRevert(
            abi.encodeWithSelector(IICS20Errors.ICS20DenomNotFound.selector, defaultPacketData.tokens[0].denom)
        );
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
        defaultPacketData.tokens[0].denom.base = erc20AddressStr;
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
        (IICS26RouterMsgs.Packet memory packet, IICS20TransferMsgs.FungibleTokenPacketDataV2 memory defaultPacketData) =
            _getDefaultPacket();

        // test invalid data
        packet.payloads[0].value = bytes("invalid");
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
        defaultPacketData.tokens[0].denom.base = "invalid";
        packet.payloads[0].value = abi.encode(defaultPacketData);
        vm.expectRevert(
            abi.encodeWithSelector(IICS20Errors.ICS20DenomNotFound.selector, defaultPacketData.tokens[0].denom)
        );
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
        defaultPacketData.tokens[0].denom.base = erc20AddressStr;
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
        (IICS26RouterMsgs.Packet memory packet, IICS20TransferMsgs.FungibleTokenPacketDataV2 memory defaultPacketData) =
            _getDefaultPacket();

        IICS20TransferMsgs.Denom memory denom =
            IICS20TransferMsgs.Denom({ base: erc20AddressStr, trace: new IICS20TransferMsgs.Hop[](1) });

        denom.trace[0] =
            IICS20TransferMsgs.Hop({ portId: packet.payloads[0].sourcePort, clientId: packet.sourceClient });

        defaultPacketData.tokens[0].denom = denom;
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
        packet.payloads[0].value = bytes("invalid");
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
        defaultPacketData.tokens[0].amount = 0;
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
        assertEq(string(ack), "{\"error\":\"invalid packet data: amount must be greater than 0\"}");
        // reset amount
        defaultPacketData.tokens[0].amount = defaultAmount;
        packet.payloads[0].value = abi.encode(defaultPacketData);

        // test receiver chain is source, but denom is not erc20 address
        IICS20TransferMsgs.Denom memory invalidErc20Denom =
            IICS20TransferMsgs.Denom({ base: "invalid", trace: new IICS20TransferMsgs.Hop[](1) });
        invalidErc20Denom.trace[0] =
            IICS20TransferMsgs.Hop({ portId: packet.payloads[0].sourcePort, clientId: packet.sourceClient });
        defaultPacketData.tokens[0].denom = invalidErc20Denom;
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
        // reset denom
        defaultPacketData.tokens[0].denom = denom;
        packet.payloads[0].value = abi.encode(defaultPacketData);

        // test with forwarding set
        defaultPacketData.forwarding.hops = new IICS20TransferMsgs.Hop[](1);
        defaultPacketData.forwarding.hops[0] = IICS20TransferMsgs.Hop({ portId: "port", clientId: "client" });
        // just to make sure we don't get a memo error
        defaultPacketData.memo = "";
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
        assertEq(string(ack), "{\"error\":\"unsupported feature: forwarding on receive\"}");
        // reset forwarding and memo
        defaultPacketData.forwarding.hops = new IICS20TransferMsgs.Hop[](0);
        defaultPacketData.memo = "memo";

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
        returns (IICS26RouterMsgs.Packet memory, IICS20TransferMsgs.FungibleTokenPacketDataV2 memory)
    {
        IICS20TransferMsgs.FungibleTokenPacketDataV2 memory defaultPacketData = _getDefaultPacketData();
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

    function _getDefaultPacketData() internal view returns (IICS20TransferMsgs.FungibleTokenPacketDataV2 memory) {
        IICS20TransferMsgs.Token[] memory tokens = new IICS20TransferMsgs.Token[](1);
        tokens[0] = IICS20TransferMsgs.Token({
            denom: IICS20TransferMsgs.Denom({ base: erc20AddressStr, trace: new IICS20TransferMsgs.Hop[](0) }),
            amount: defaultAmount
        });
        IICS20TransferMsgs.FungibleTokenPacketDataV2 memory defaultPacketData = IICS20TransferMsgs
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

        return defaultPacketData;
    }

    function generateLongString(uint256 length) internal pure returns (string memory) {
        bytes memory bytesArray = new bytes(length);
        for (uint256 i = 0; i < length; i++) {
            bytesArray[i] = bytes1(uint8(97)); // ASCII 'a'
        }
        return string(bytesArray);
    }
}
