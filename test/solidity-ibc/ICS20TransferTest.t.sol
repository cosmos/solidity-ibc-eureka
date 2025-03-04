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
import { IIBCUUPSUpgradeable } from "../../contracts/interfaces/IIBCUUPSUpgradeable.sol";

import { ICS20Transfer } from "../../contracts/ICS20Transfer.sol";
import { TestERC20, MalfunctioningERC20 } from "./mocks/TestERC20.sol";
import { ICS20Lib } from "../../contracts/utils/ICS20Lib.sol";
import { ICS24Host } from "../../contracts/utils/ICS24Host.sol";
import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";
import { ERC1967Proxy } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Proxy.sol";
import { IBCERC20 } from "../../contracts/utils/IBCERC20.sol";
import { Escrow } from "../../contracts/utils/Escrow.sol";
import { ISignatureTransfer } from "@uniswap/permit2/src/interfaces/ISignatureTransfer.sol";
import { DeployPermit2 } from "@uniswap/permit2/test/utils/DeployPermit2.sol";
import { PermitSignature } from "./utils/PermitSignature.sol";

contract ICS20TransferTest is Test, DeployPermit2, PermitSignature {
    ICS20Transfer public ics20Transfer;
    TestERC20 public erc20;
    ISignatureTransfer public permit2;

    address public sender;
    string public senderStr;
    uint256 public senderKey;
    address public receiver;
    string public receiverStr;

    /// @dev the default send amount for sendTransfer
    uint256 public defaultAmount = 1_000_000_100_000_000_001;

    function setUp() public {
        permit2 = ISignatureTransfer(deployPermit2());
        address escrowLogic = address(new Escrow());
        address ibcERC20Logic = address(new IBCERC20());
        ICS20Transfer ics20TransferLogic = new ICS20Transfer();

        ERC1967Proxy transferProxy = new ERC1967Proxy(
            address(ics20TransferLogic),
            abi.encodeCall(
                ICS20Transfer.initialize, (address(this), escrowLogic, ibcERC20Logic, address(0), address(permit2))
            )
        );

        ics20Transfer = ICS20Transfer(address(transferProxy));
        assertEq(ics20Transfer.getPermit2(), address(permit2));
        assertEq(ics20Transfer.ics26(), address(this));

        erc20 = new TestERC20();
        assertEq(ics20Transfer.ibcERC20Denom(address(erc20)), "");

        (sender, senderKey) = makeAddrAndKey("sender");
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
        uint64 sequence = ics20Transfer.sendTransfer(msgSendTransfer);
        assertEq(sequence, 42);
    }

    function test_failure_sendTransfer() public {
        // this contract acts as the ics26Router
        vm.mockCall(address(this), abi.encodeWithSelector(IICS26Router.sendPacket.selector), abi.encode(uint32(42)));

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
        TestERC20 tmpERC20 = erc20;
        erc20 = malfunctioningERC20;

        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20UnexpectedERC20Balance.selector, defaultAmount, 0));
        vm.prank(sender);
        ics20Transfer.sendTransfer(_getTestSendTransferMsg());

        // prove that it works with valid data
        erc20 = tmpERC20;
        vm.prank(sender);
        ics20Transfer.sendTransfer(_getTestSendTransferMsg());
    }

    function test_failure_sendTransferWithPermit2() public {
        // this contract acts as the ics26Router
        vm.mockCall(address(this), abi.encodeWithSelector(IICS26Router.sendPacket.selector), abi.encode(uint32(42)));

        ISignatureTransfer.PermitTransferFrom memory permit = ISignatureTransfer.PermitTransferFrom({
            permitted: ISignatureTransfer.TokenPermissions({ token: address(erc20), amount: defaultAmount }),
            nonce: 0,
            deadline: block.timestamp + 100
        });
        bytes memory signature =
            this.getPermitTransferSignature(permit, senderKey, address(ics20Transfer), permit2.DOMAIN_SEPARATOR());

        // test missing approval
        vm.expectRevert("TRANSFER_FROM_FAILED");
        vm.prank(sender);
        ics20Transfer.sendTransferWithPermit2(_getTestSendTransferMsg(), permit, signature);

        // mint and approve permit2
        erc20.mint(sender, defaultAmount);
        vm.prank(sender);
        erc20.approve(address(permit2), defaultAmount);

        // test invalid amount
        defaultAmount = 0;
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20InvalidAmount.selector, 0));
        vm.prank(sender);
        ics20Transfer.sendTransferWithPermit2(_getTestSendTransferMsg(), permit, signature);
        // reset amount
        defaultAmount = 1_000_000_100_000_000_001;

        // test different permit and send token
        TestERC20 differentERC20 = new TestERC20();
        differentERC20.mint(sender, defaultAmount);
        vm.prank(sender);
        differentERC20.approve(address(permit2), defaultAmount);
        ISignatureTransfer.PermitTransferFrom memory differentPermit = ISignatureTransfer.PermitTransferFrom({
            permitted: ISignatureTransfer.TokenPermissions({ token: address(differentERC20), amount: defaultAmount }),
            nonce: 1,
            deadline: block.timestamp + 100
        });
        bytes memory differentSignature = this.getPermitTransferSignature(
            differentPermit, senderKey, address(ics20Transfer), permit2.DOMAIN_SEPARATOR()
        );
        vm.expectRevert(
            abi.encodeWithSelector(
                IICS20Errors.ICS20Permit2TokenMismatch.selector, address(differentERC20), address(erc20)
            )
        );
        vm.prank(sender);
        ics20Transfer.sendTransferWithPermit2(_getTestSendTransferMsg(), differentPermit, differentSignature);

        // test invalid signature
        bytes memory invalidSignature = new bytes(65);
        vm.expectRevert();
        vm.prank(sender);
        ics20Transfer.sendTransferWithPermit2(_getTestSendTransferMsg(), permit, invalidSignature);

        // prove that it works with a valid signature
        vm.prank(sender);
        ics20Transfer.sendTransferWithPermit2(_getTestSendTransferMsg(), permit, signature);
    }

    function test_success_sendTransferWithSender() public {
        address customSender = makeAddr("customSender");

        // give permission to the delegate sender
        vm.mockCall(address(this), IIBCUUPSUpgradeable.isAdmin.selector, abi.encode(true));
        ics20Transfer.grantDelegateSenderRole(sender);

        (IICS26RouterMsgs.Packet memory packet, IICS20TransferMsgs.FungibleTokenPacketData memory expPacketData) =
            _getDefaultPacket();
        expPacketData.sender = Strings.toHexString(customSender);

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

        vm.prank(sender);
        vm.expectCall(
            address(this),
            abi.encodeCall(
                IICS26Router.sendPacket,
                IICS26RouterMsgs.MsgSendPacket({
                    sourceClient: msgSendTransfer.sourceClient,
                    timeoutTimestamp: msgSendTransfer.timeoutTimestamp,
                    payload: IICS26RouterMsgs.Payload({
                        sourcePort: ICS20Lib.DEFAULT_PORT_ID,
                        destPort: ICS20Lib.DEFAULT_PORT_ID,
                        version: ICS20Lib.ICS20_VERSION,
                        encoding: ICS20Lib.ICS20_ENCODING,
                        value: abi.encode(expPacketData)
                    })
                })
            )
        );
        vm.mockCall(address(this), abi.encodeWithSelector(IICS26Router.sendPacket.selector), abi.encode(uint32(42)));
        uint64 sequence = ics20Transfer.sendTransferWithSender(msgSendTransfer, customSender);
        assertEq(sequence, 42);
    }

    function test_failure_onAcknowledgementPacket() public {
        (IICS26RouterMsgs.Packet memory packet, IICS20TransferMsgs.FungibleTokenPacketData memory defaultPacketData) =
            _getDefaultPacket();

        // cheat the escrow mapping to not error on finding the escrow
        bytes32 someAddress = keccak256("someAddress");
        vm.store(address(ics20Transfer), _getEscrowMappingSlot(packet.sourceClient), someAddress);

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

        // cheat the escrow mapping to not error on finding the escrow
        bytes32 someAddress = keccak256("someAddress");
        vm.store(address(ics20Transfer), _getEscrowMappingSlot(packet.sourceClient), someAddress);

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

        // test invalid source port
        packet.payloads[0].sourcePort = "invalid";
        vm.expectRevert(
            abi.encodeWithSelector(IICS20Errors.ICS20InvalidPort.selector, ICS20Lib.DEFAULT_PORT_ID, "invalid")
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
        // reset source port
        packet.payloads[0].sourcePort = ICS20Lib.DEFAULT_PORT_ID;

        // test invalid dest port
        packet.payloads[0].destPort = "invalid";
        vm.expectRevert(
            abi.encodeWithSelector(IICS20Errors.ICS20InvalidPort.selector, ICS20Lib.DEFAULT_PORT_ID, "invalid")
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
        // reset dest port
        packet.payloads[0].destPort = ICS20Lib.DEFAULT_PORT_ID;

        // test invalid encoding
        packet.payloads[0].encoding = "invalid";
        vm.expectRevert(
            abi.encodeWithSelector(IICS20Errors.ICS20UnexpectedEncoding.selector, ICS20Lib.ICS20_ENCODING, "invalid")
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
        // reset encoding
        packet.payloads[0].encoding = ICS20Lib.ICS20_ENCODING;
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
            sourcePort: ICS20Lib.DEFAULT_PORT_ID,
            destPort: ICS20Lib.DEFAULT_PORT_ID,
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
            destPort: ICS20Lib.DEFAULT_PORT_ID,
            timeoutTimestamp: 0,
            memo: "memo"
        });
    }

    function _getEscrowMappingSlot(string memory clientId) internal pure returns (bytes32) {
        bytes32 ics20Slot = 0x823f7a8ea9ae6df0eb03ec5e1682d7a2839417ad8a91774118e6acf2e8d2f800;
        return keccak256(abi.encodePacked(clientId, ics20Slot));
    }
}
