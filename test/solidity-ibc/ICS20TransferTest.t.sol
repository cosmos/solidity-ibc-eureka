// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

// solhint-disable custom-errors,max-line-length,function-max-lines

import { Test } from "forge-std/Test.sol";

import { IICS26RouterMsgs } from "../../contracts/msgs/IICS26RouterMsgs.sol";
import { IICS20TransferMsgs } from "../../contracts/msgs/IICS20TransferMsgs.sol";

import { IICS20Errors } from "../../contracts/errors/IICS20Errors.sol";
import { IIBCAppCallbacks } from "../../contracts/msgs/IIBCAppCallbacks.sol";
import { IERC20Errors } from "@openzeppelin-contracts/interfaces/draft-IERC6093.sol";
import { IICS26Router } from "../../contracts/interfaces/IICS26Router.sol";
import { IIBCSenderCallbacks } from "../../contracts/interfaces/IIBCSenderCallbacks.sol";

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
import { CallbackReceiver } from "./mocks/CallbackReceiver.sol";
import { AccessManager } from "@openzeppelin-contracts/access/manager/AccessManager.sol";
import { IBCRolesLib } from "../../contracts/utils/IBCRolesLib.sol";
import { TestHelper } from "./utils/TestHelper.sol";

contract ICS20TransferTest is Test, DeployPermit2, PermitSignature {
    ICS20Transfer public ics20Transfer;
    TestERC20 public erc20;
    ISignatureTransfer public permit2;
    AccessManager public accessManager;

    address public ics26 = makeAddr("ics26router");
    TestHelper public th = new TestHelper();

    function setUp() public {
        permit2 = ISignatureTransfer(deployPermit2());
        address escrowLogic = address(new Escrow());
        address ibcERC20Logic = address(new IBCERC20());
        ICS20Transfer ics20TransferLogic = new ICS20Transfer();

        accessManager = new AccessManager(address(this));
        ERC1967Proxy transferProxy = new ERC1967Proxy(
            address(ics20TransferLogic),
            abi.encodeCall(
                ICS20Transfer.initialize, (ics26, escrowLogic, ibcERC20Logic, address(permit2), address(accessManager))
            )
        );

        ics20Transfer = ICS20Transfer(address(transferProxy));
        assertEq(ics20Transfer.getPermit2(), address(permit2));
        assertEq(ics20Transfer.ics26(), ics26);

        erc20 = new TestERC20();
        assertEq(ics20Transfer.ibcERC20Denom(address(erc20)), "");
    }

    function testFuzz_success_sendTransfer(uint256 amount, uint64 seq, uint64 timeoutTimestamp) public {
        vm.assume(amount > 0);

        string memory sourceClient = th.randomString();
        string memory destClient = th.randomString();
        string memory memo = th.randomString();
        string memory receiver = th.randomString();

        address sender = makeAddr("sender");

        IICS26RouterMsgs.Packet memory expPacket = IICS26RouterMsgs.Packet({
            sequence: seq,
            sourceClient: sourceClient,
            destClient: destClient,
            timeoutTimestamp: timeoutTimestamp,
            payloads: new IICS26RouterMsgs.Payload[](1)
        });
        expPacket.payloads[0] = IICS26RouterMsgs.Payload({
            sourcePort: ICS20Lib.DEFAULT_PORT_ID,
            destPort: ICS20Lib.DEFAULT_PORT_ID,
            version: ICS20Lib.ICS20_VERSION,
            encoding: ICS20Lib.ICS20_ENCODING,
            value: abi.encode(
                IICS20TransferMsgs.FungibleTokenPacketData({
                    denom: Strings.toHexString(address(erc20)),
                    amount: amount,
                    sender: Strings.toHexString(sender),
                    receiver: receiver,
                    memo: memo
                })
            )
        });

        erc20.mint(sender, amount);
        vm.prank(sender);
        erc20.approve(address(ics20Transfer), amount);

        IICS20TransferMsgs.SendTransferMsg memory msgSendTransfer = IICS20TransferMsgs.SendTransferMsg({
            denom: address(erc20),
            amount: amount,
            receiver: receiver,
            sourceClient: sourceClient,
            destPort: destClient,
            timeoutTimestamp: timeoutTimestamp,
            memo: memo
        });

        vm.mockCall(ics26, IICS26Router.sendPacket.selector, abi.encode(seq));
        vm.expectCall(
            ics26,
            abi.encodeCall(
                IICS26Router.sendPacket,
                IICS26RouterMsgs.MsgSendPacket({
                    sourceClient: sourceClient,
                    timeoutTimestamp: timeoutTimestamp,
                    payload: expPacket.payloads[0]
                })
            )
        );
        vm.prank(sender);
        uint64 sequence = ics20Transfer.sendTransfer(msgSendTransfer);
        assertEq(sequence, seq);
    }

    function testFuzz_failure_sendTransfer(uint256 amount, uint64 seq, uint64 timeoutTimestamp) public {
        vm.assume(amount > 0);

        vm.mockCall(ics26, IICS26Router.sendPacket.selector, abi.encode(seq));

        string memory sourceClient = th.randomString();
        string memory destClient = th.randomString();
        string memory memo = th.randomString();
        string memory receiver = th.randomString();

        address sender = makeAddr("sender");

        IICS20TransferMsgs.SendTransferMsg memory msgSendTransfer = IICS20TransferMsgs.SendTransferMsg({
            denom: address(erc20),
            amount: amount,
            receiver: receiver,
            sourceClient: sourceClient,
            destPort: destClient,
            timeoutTimestamp: timeoutTimestamp,
            memo: memo
        });

        // ===== Case 1: Test missing approval =====
        vm.expectRevert(
            abi.encodeWithSelector(IERC20Errors.ERC20InsufficientAllowance.selector, address(ics20Transfer), 0, amount)
        );
        ics20Transfer.sendTransfer(msgSendTransfer);

        // ===== Case 2: Test insufficient balance =====
        vm.prank(sender);
        erc20.approve(address(ics20Transfer), amount);
        vm.expectRevert(abi.encodeWithSelector(IERC20Errors.ERC20InsufficientBalance.selector, sender, 0, amount));
        vm.prank(sender);
        ics20Transfer.sendTransfer(msgSendTransfer);

        erc20.mint(sender, amount);

        // ===== Case 3: Empty amount =====
        msgSendTransfer.amount = 0;
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20InvalidAmount.selector, 0));
        vm.prank(sender);
        ics20Transfer.sendTransfer(msgSendTransfer);

        // reset amount
        msgSendTransfer.amount = amount;

        // ===== Case 4: Malfunctioning ERC20 =====
        MalfunctioningERC20 malfunctioningERC20 = new MalfunctioningERC20();
        malfunctioningERC20.mint(sender, amount);
        malfunctioningERC20.setMalfunction(true); // Turn on the malfunctioning behaviour (no update)
        vm.prank(sender);
        malfunctioningERC20.approve(address(ics20Transfer), amount);

        msgSendTransfer.denom = address(malfunctioningERC20);

        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20UnexpectedERC20Balance.selector, amount, 0));
        vm.prank(sender);
        ics20Transfer.sendTransfer(msgSendTransfer);
    }

    function testFuzz_failure_sendTransferWithPermit2(uint256 amount, uint64 seq, uint64 timeoutTimestamp) public {
        vm.assume(amount > 0);

        (address sender, uint256 senderKey) = makeAddrAndKey("sender");
        string memory sourceClient = th.randomString();
        string memory destClient = th.randomString();
        string memory memo = th.randomString();
        string memory receiver = th.randomString();

        IICS20TransferMsgs.SendTransferMsg memory msgSendTransfer = IICS20TransferMsgs.SendTransferMsg({
            denom: address(erc20),
            amount: amount,
            receiver: receiver,
            sourceClient: sourceClient,
            destPort: destClient,
            timeoutTimestamp: timeoutTimestamp,
            memo: memo
        });

        // this contract acts as the ics26Router
        vm.mockCall(ics26, IICS26Router.sendPacket.selector, abi.encode(seq));

        ISignatureTransfer.PermitTransferFrom memory permit = ISignatureTransfer.PermitTransferFrom({
            permitted: ISignatureTransfer.TokenPermissions({ token: address(erc20), amount: amount }),
            nonce: 0,
            deadline: block.timestamp + 100
        });
        bytes memory signature =
            this.getPermitTransferSignature(permit, senderKey, address(ics20Transfer), permit2.DOMAIN_SEPARATOR());

        // ===== Case 1: Missing Approval =====
        vm.expectRevert("TRANSFER_FROM_FAILED");
        vm.prank(sender);
        ics20Transfer.sendTransferWithPermit2(msgSendTransfer, permit, signature);

        // ===== Case 2: Mint and Approve permit2 =====
        erc20.mint(sender, amount);
        vm.prank(sender);
        erc20.approve(address(permit2), amount);

        // ===== Case 3: Invalid Amount =====
        msgSendTransfer.amount = 0;
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20InvalidAmount.selector, 0));
        vm.prank(sender);
        ics20Transfer.sendTransferWithPermit2(msgSendTransfer, permit, signature);
        // reset amount
        msgSendTransfer.amount = amount;

        // ===== Case 4: Permit and Token Mismatch =====
        TestERC20 differentERC20 = new TestERC20();
        differentERC20.mint(sender, amount);
        vm.prank(sender);
        differentERC20.approve(address(permit2), amount);
        ISignatureTransfer.PermitTransferFrom memory differentPermit = ISignatureTransfer.PermitTransferFrom({
            permitted: ISignatureTransfer.TokenPermissions({ token: address(differentERC20), amount: amount }),
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
        ics20Transfer.sendTransferWithPermit2(msgSendTransfer, differentPermit, differentSignature);

        // ===== Case 4: Invalid Signature =====
        bytes memory invalidSignature = new bytes(65);
        vm.expectRevert();
        vm.prank(sender);
        ics20Transfer.sendTransferWithPermit2(msgSendTransfer, permit, invalidSignature);

        // TODO: prove that it works with a valid signature
        vm.prank(sender);
        ics20Transfer.sendTransferWithPermit2(msgSendTransfer, permit, signature);
    }

    function testFuzz_success_sendTransferWithSender(uint256 amount, uint64 seq, uint64 timeoutTimestamp) public {
        vm.assume(amount > 0);

        address sender = makeAddr("sender");
        address customSender = makeAddr("customSender");
        string memory sourceClient = th.randomString();
        string memory destClient = th.randomString();
        string memory memo = th.randomString();
        string memory receiver = th.randomString();

        IICS26RouterMsgs.Packet memory expPacket = IICS26RouterMsgs.Packet({
            sequence: seq,
            sourceClient: sourceClient,
            destClient: destClient,
            timeoutTimestamp: timeoutTimestamp,
            payloads: new IICS26RouterMsgs.Payload[](1)
        });
        expPacket.payloads[0] = IICS26RouterMsgs.Payload({
            sourcePort: ICS20Lib.DEFAULT_PORT_ID,
            destPort: ICS20Lib.DEFAULT_PORT_ID,
            version: ICS20Lib.ICS20_VERSION,
            encoding: ICS20Lib.ICS20_ENCODING,
            value: abi.encode(
                IICS20TransferMsgs.FungibleTokenPacketData({
                    denom: Strings.toHexString(address(erc20)),
                    amount: amount,
                    sender: Strings.toHexString(customSender),
                    receiver: receiver,
                    memo: memo
                })
            )
        });

        // give permission to the delegate sender
        accessManager.grantRole(IBCRolesLib.DELEGATE_SENDER_ROLE, sender, 0);
        accessManager.setTargetFunctionRole(
            address(ics20Transfer), IBCRolesLib.delegateSenderSelectors(), IBCRolesLib.DELEGATE_SENDER_ROLE
        );

        erc20.mint(sender, amount);
        vm.prank(sender);
        erc20.approve(address(ics20Transfer), amount);

        IICS20TransferMsgs.SendTransferMsg memory msgSendTransfer = IICS20TransferMsgs.SendTransferMsg({
            denom: address(erc20),
            amount: amount,
            receiver: receiver,
            sourceClient: sourceClient,
            destPort: expPacket.payloads[0].sourcePort,
            timeoutTimestamp: timeoutTimestamp,
            memo: memo
        });

        vm.prank(sender);
        vm.expectCall(
            ics26,
            abi.encodeCall(
                IICS26Router.sendPacket,
                IICS26RouterMsgs.MsgSendPacket({
                    sourceClient: msgSendTransfer.sourceClient,
                    timeoutTimestamp: msgSendTransfer.timeoutTimestamp,
                    payload: expPacket.payloads[0]
                })
            )
        );
        vm.mockCall(ics26, IICS26Router.sendPacket.selector, abi.encode(seq));
        uint64 sequence = ics20Transfer.sendTransferWithSender(msgSendTransfer, customSender);
        assertEq(sequence, seq);
    }

    function testFuzz_success_onAcknowledgementPacketCallback(
        uint256 amount,
        uint64 seq,
        uint64 timeoutTimestamp
    )
        public
    {
        // override sender
        address sender = address(new CallbackReceiver());

        string memory sourceClient = th.randomString();
        string memory destClient = th.randomString();
        string memory memo = th.randomString();
        string memory receiver = th.randomString();
        address relayer = makeAddr("relayer");

        IICS26RouterMsgs.Packet memory expPacket = IICS26RouterMsgs.Packet({
            sequence: seq,
            sourceClient: sourceClient,
            destClient: destClient,
            timeoutTimestamp: timeoutTimestamp,
            payloads: new IICS26RouterMsgs.Payload[](1)
        });
        expPacket.payloads[0] = IICS26RouterMsgs.Payload({
            sourcePort: ICS20Lib.DEFAULT_PORT_ID,
            destPort: ICS20Lib.DEFAULT_PORT_ID,
            version: ICS20Lib.ICS20_VERSION,
            encoding: ICS20Lib.ICS20_ENCODING,
            value: abi.encode(
                IICS20TransferMsgs.FungibleTokenPacketData({
                    denom: Strings.toHexString(address(erc20)),
                    amount: amount,
                    sender: Strings.toHexString(sender),
                    receiver: receiver,
                    memo: memo
                })
            )
        });

        // cheat the escrow mapping to not error on finding the escrow
        bytes32 someAddress = keccak256("someAddress");
        vm.store(address(ics20Transfer), _getEscrowMappingSlot(sourceClient), someAddress);

        IIBCAppCallbacks.OnAcknowledgementPacketCallback memory callbackMsg = IIBCAppCallbacks
            .OnAcknowledgementPacketCallback({
            sourceClient: sourceClient,
            destinationClient: destClient,
            sequence: seq,
            payload: expPacket.payloads[0],
            acknowledgement: ICS20Lib.SUCCESSFUL_ACKNOWLEDGEMENT_JSON,
            relayer: relayer
        });

        // Test success ack with callback
        vm.expectCall(sender, abi.encodeCall(IIBCSenderCallbacks.onAckPacket, (true, callbackMsg)));
        vm.prank(ics26);
        ics20Transfer.onAcknowledgementPacket(callbackMsg);

        // Test error ack with callback
        address escrowAddress = address(uint160(uint256(someAddress)));
        callbackMsg.acknowledgement = abi.encodePacked(ICS24Host.UNIVERSAL_ERROR_ACK);
        vm.mockCall(escrowAddress, Escrow.recvCallback.selector, bytes(""));
        vm.expectCall(sender, abi.encodeCall(IIBCSenderCallbacks.onAckPacket, (false, callbackMsg)));
        vm.prank(ics26);
        ics20Transfer.onAcknowledgementPacket(callbackMsg);
    }

    function testFuzz_failure_onAcknowledgementPacket(uint256 amount, uint64 seq) public {
        address sender = makeAddr("sender");
        string memory sourceClient = th.randomString();
        string memory destClient = th.randomString();
        string memory memo = th.randomString();
        string memory receiver = th.randomString();
        address relayer = makeAddr("relayer");

        IIBCAppCallbacks.OnAcknowledgementPacketCallback memory callbackMsg = IIBCAppCallbacks
            .OnAcknowledgementPacketCallback({
            sourceClient: sourceClient,
            destinationClient: destClient,
            sequence: seq,
            payload: IICS26RouterMsgs.Payload({
                sourcePort: ICS20Lib.DEFAULT_PORT_ID,
                destPort: ICS20Lib.DEFAULT_PORT_ID,
                version: ICS20Lib.ICS20_VERSION,
                encoding: ICS20Lib.ICS20_ENCODING,
                value: abi.encode(
                    IICS20TransferMsgs.FungibleTokenPacketData({
                        denom: Strings.toHexString(address(erc20)),
                        amount: amount,
                        sender: Strings.toHexString(sender),
                        receiver: receiver,
                        memo: memo
                    })
                )
            }),
            acknowledgement: ICS24Host.UNIVERSAL_ERROR_ACK,
            relayer: relayer
        });

        // cheat the escrow mapping to not error on finding the escrow
        bytes32 someAddress = keccak256("someAddress");
        vm.store(address(ics20Transfer), _getEscrowMappingSlot(sourceClient), someAddress);

        // ===== Case 1: Invalid Data =====
        bytes memory data = bytes("invalid");
        callbackMsg.payload.value = data;
        vm.expectRevert();
        vm.prank(ics26);
        ics20Transfer.onAcknowledgementPacket(callbackMsg);
        // reset data
        callbackMsg.payload.value = abi.encode(
            IICS20TransferMsgs.FungibleTokenPacketData({
                denom: Strings.toHexString(address(erc20)),
                amount: amount,
                sender: Strings.toHexString(sender),
                receiver: receiver,
                memo: memo
            })
        );

        // ===== Case 2: Invalid contract/denom =====
        callbackMsg.payload.value = abi.encode(
            IICS20TransferMsgs.FungibleTokenPacketData({
                denom: th.INVALID_ID(),
                amount: amount,
                sender: Strings.toHexString(sender),
                receiver: receiver,
                memo: memo
            })
        );
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20InvalidAddress.selector, th.INVALID_ID()));
        vm.prank(ics26);
        ics20Transfer.onAcknowledgementPacket(callbackMsg);
        // reset denom
        callbackMsg.payload.value = abi.encode(
            IICS20TransferMsgs.FungibleTokenPacketData({
                denom: Strings.toHexString(address(erc20)),
                amount: amount,
                sender: Strings.toHexString(sender),
                receiver: receiver,
                memo: memo
            })
        );

        // ===== Case 4: Denom not found for a non-native token (source trace) =====
        string memory missingDenom =
            string(abi.encodePacked(callbackMsg.payload.sourcePort, "/", callbackMsg.sourceClient, "/", "notfound"));
        callbackMsg.payload.value = abi.encode(
            IICS20TransferMsgs.FungibleTokenPacketData({
                denom: missingDenom,
                amount: amount,
                sender: Strings.toHexString(sender),
                receiver: receiver,
                memo: memo
            })
        );
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20DenomNotFound.selector, missingDenom));
        vm.prank(ics26);
        ics20Transfer.onAcknowledgementPacket(callbackMsg);
        // reset denom
        callbackMsg.payload.value = abi.encode(
            IICS20TransferMsgs.FungibleTokenPacketData({
                denom: Strings.toHexString(address(erc20)),
                amount: amount,
                sender: Strings.toHexString(sender),
                receiver: receiver,
                memo: memo
            })
        );

        // ===== Case 5: Invalid Sender =====
        callbackMsg.payload.value = abi.encode(
            IICS20TransferMsgs.FungibleTokenPacketData({
                denom: Strings.toHexString(address(erc20)),
                amount: amount,
                sender: th.INVALID_ID(),
                receiver: receiver,
                memo: memo
            })
        );
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20InvalidAddress.selector, th.INVALID_ID()));
        vm.prank(ics26);
        ics20Transfer.onAcknowledgementPacket(callbackMsg);
        // reset sender
        callbackMsg.payload.value = abi.encode(
            IICS20TransferMsgs.FungibleTokenPacketData({
                denom: Strings.toHexString(address(erc20)),
                amount: amount,
                sender: Strings.toHexString(sender),
                receiver: receiver,
                memo: memo
            })
        );
    }

    function testFuzz_success_onTimeoutPacketCallback(uint256 amount, uint64 seq) public {
        address sender = address(new CallbackReceiver());
        string memory sourceClient = th.randomString();
        string memory destClient = th.randomString();
        string memory memo = th.randomString();
        string memory receiver = th.randomString();
        address relayer = makeAddr("relayer");

        IIBCAppCallbacks.OnTimeoutPacketCallback memory callbackMsg = IIBCAppCallbacks.OnTimeoutPacketCallback({
            sourceClient: sourceClient,
            destinationClient: destClient,
            sequence: seq,
            payload: IICS26RouterMsgs.Payload({
                sourcePort: ICS20Lib.DEFAULT_PORT_ID,
                destPort: ICS20Lib.DEFAULT_PORT_ID,
                version: ICS20Lib.ICS20_VERSION,
                encoding: ICS20Lib.ICS20_ENCODING,
                value: abi.encode(
                    IICS20TransferMsgs.FungibleTokenPacketData({
                        denom: Strings.toHexString(address(erc20)),
                        amount: amount,
                        sender: Strings.toHexString(sender),
                        receiver: receiver,
                        memo: memo
                    })
                )
            }),
            relayer: relayer
        });

        // cheat the escrow mapping to not error on finding the escrow
        bytes32 someAddress = keccak256("someAddress");
        vm.store(address(ics20Transfer), _getEscrowMappingSlot(sourceClient), someAddress);

        // Test success timeout with callback
        address escrowAddress = address(uint160(uint256(someAddress)));
        vm.mockCall(escrowAddress, Escrow.recvCallback.selector, bytes(""));
        vm.expectCall(sender, abi.encodeCall(IIBCSenderCallbacks.onTimeoutPacket, (callbackMsg)));
        vm.prank(ics26);
        ics20Transfer.onTimeoutPacket(callbackMsg);
    }

    function testFuzz_failure_onTimeoutPacket(uint256 amount, uint64 seq) public {
        address sender = makeAddr("sender");
        string memory sourceClient = th.randomString();
        string memory destClient = th.randomString();
        string memory memo = th.randomString();
        string memory receiver = th.randomString();
        address relayer = makeAddr("relayer");

        IIBCAppCallbacks.OnTimeoutPacketCallback memory callbackMsg = IIBCAppCallbacks.OnTimeoutPacketCallback({
            sourceClient: sourceClient,
            destinationClient: destClient,
            sequence: seq,
            payload: IICS26RouterMsgs.Payload({
                sourcePort: ICS20Lib.DEFAULT_PORT_ID,
                destPort: ICS20Lib.DEFAULT_PORT_ID,
                version: ICS20Lib.ICS20_VERSION,
                encoding: ICS20Lib.ICS20_ENCODING,
                value: abi.encode(
                    IICS20TransferMsgs.FungibleTokenPacketData({
                        denom: Strings.toHexString(address(erc20)),
                        amount: amount,
                        sender: Strings.toHexString(sender),
                        receiver: receiver,
                        memo: memo
                    })
                )
            }),
            relayer: relayer
        });

        // cheat the escrow mapping to not error on finding the escrow
        bytes32 someAddress = keccak256("someAddress");
        vm.store(address(ics20Transfer), _getEscrowMappingSlot(sourceClient), someAddress);

        // ===== Case 1: Invalid Data
        callbackMsg.payload.value = bytes("invalid");
        vm.expectRevert(bytes(""));
        vm.prank(ics26);
        ics20Transfer.onTimeoutPacket(callbackMsg);
        // reset data
        callbackMsg.payload.value = abi.encode(
            IICS20TransferMsgs.FungibleTokenPacketData({
                denom: Strings.toHexString(address(erc20)),
                amount: amount,
                sender: Strings.toHexString(sender),
                receiver: receiver,
                memo: memo
            })
        );

        // ===== Case 2: Invalid ERC20 Denom =====
        callbackMsg.payload.value = abi.encode(
            IICS20TransferMsgs.FungibleTokenPacketData({
                denom: th.INVALID_ID(),
                amount: amount,
                sender: Strings.toHexString(sender),
                receiver: receiver,
                memo: memo
            })
        );
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20InvalidAddress.selector, th.INVALID_ID()));
        vm.prank(ics26);
        ics20Transfer.onTimeoutPacket(callbackMsg);
        // reset denom
        callbackMsg.payload.value = abi.encode(
            IICS20TransferMsgs.FungibleTokenPacketData({
                denom: Strings.toHexString(address(erc20)),
                amount: amount,
                sender: Strings.toHexString(sender),
                receiver: receiver,
                memo: memo
            })
        );

        // ===== Case 3: Denom not found for a non-native token (source trace) =====
        string memory invalidDenom =
            string(abi.encodePacked(callbackMsg.payload.sourcePort, "/", callbackMsg.sourceClient, "/", "notfound"));
        callbackMsg.payload.value = abi.encode(
            IICS20TransferMsgs.FungibleTokenPacketData({
                denom: invalidDenom,
                amount: amount,
                sender: Strings.toHexString(sender),
                receiver: receiver,
                memo: memo
            })
        );
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20DenomNotFound.selector, invalidDenom));
        vm.prank(ics26);
        ics20Transfer.onTimeoutPacket(callbackMsg);
        // reset denom
        callbackMsg.payload.value = abi.encode(
            IICS20TransferMsgs.FungibleTokenPacketData({
                denom: Strings.toHexString(address(erc20)),
                amount: amount,
                sender: Strings.toHexString(sender),
                receiver: receiver,
                memo: memo
            })
        );

        // ===== Case 4: Invalid Sender =====
        callbackMsg.payload.value = abi.encode(
            IICS20TransferMsgs.FungibleTokenPacketData({
                denom: Strings.toHexString(address(erc20)),
                amount: amount,
                sender: th.INVALID_ID(),
                receiver: receiver,
                memo: memo
            })
        );
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20InvalidAddress.selector, th.INVALID_ID()));
        vm.prank(ics26);
        ics20Transfer.onTimeoutPacket(callbackMsg);
        // reset sender
        callbackMsg.payload.value = abi.encode(
            IICS20TransferMsgs.FungibleTokenPacketData({
                denom: Strings.toHexString(address(erc20)),
                amount: amount,
                sender: Strings.toHexString(sender),
                receiver: receiver,
                memo: memo
            })
        );
    }

    function testFuzz_failure_onRecvPacket(uint256 amount, uint64 seq) public {
        vm.assume(amount > 0);

        address sender = makeAddr("sender");
        string memory sourceClient = th.randomString();
        string memory destClient = th.randomString();
        string memory memo = th.randomString();
        string memory receiver = Strings.toHexString(makeAddr("receiver"));
        address relayer = makeAddr("relayer");

        string memory denom = string(
            abi.encodePacked(ICS20Lib.DEFAULT_PORT_ID, "/", sourceClient, "/", Strings.toHexString(address(erc20)))
        );
        IIBCAppCallbacks.OnRecvPacketCallback memory callbackMsg = IIBCAppCallbacks.OnRecvPacketCallback({
            sourceClient: sourceClient,
            destinationClient: destClient,
            sequence: seq,
            payload: IICS26RouterMsgs.Payload({
                sourcePort: ICS20Lib.DEFAULT_PORT_ID,
                destPort: ICS20Lib.DEFAULT_PORT_ID,
                version: ICS20Lib.ICS20_VERSION,
                encoding: ICS20Lib.ICS20_ENCODING,
                value: abi.encode(
                    IICS20TransferMsgs.FungibleTokenPacketData({
                        denom: denom,
                        amount: amount,
                        sender: Strings.toHexString(sender),
                        receiver: receiver,
                        memo: memo
                    })
                )
            }),
            relayer: relayer
        });

        // ===== Case 1: Invalid Version =====
        callbackMsg.payload.version = th.INVALID_ID();
        vm.expectRevert(
            abi.encodeWithSelector(
                IICS20Errors.ICS20UnexpectedVersion.selector, ICS20Lib.ICS20_VERSION, th.INVALID_ID()
            )
        );
        vm.prank(ics26);
        ics20Transfer.onRecvPacket(callbackMsg);
        // Reset version
        callbackMsg.payload.version = ICS20Lib.ICS20_VERSION;

        // ===== Case 2: Invalid Data =====
        callbackMsg.payload.value = bytes("invalid");
        vm.expectRevert(); // here we expect a generic revert caused by the abi.decodePayload function
        vm.prank(ics26);
        ics20Transfer.onRecvPacket(callbackMsg);
        // reset data
        callbackMsg.payload.value = abi.encode(
            IICS20TransferMsgs.FungibleTokenPacketData({
                denom: denom,
                amount: amount,
                sender: Strings.toHexString(sender),
                receiver: receiver,
                memo: memo
            })
        );

        // ===== Case 3: Invalid Amount =====
        callbackMsg.payload.value = abi.encode(
            IICS20TransferMsgs.FungibleTokenPacketData({
                denom: denom,
                amount: 0,
                sender: Strings.toHexString(sender),
                receiver: receiver,
                memo: memo
            })
        );
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20InvalidAmount.selector, 0));
        vm.prank(ics26);
        ics20Transfer.onRecvPacket(callbackMsg);
        // reset amount
        callbackMsg.payload.value = abi.encode(
            IICS20TransferMsgs.FungibleTokenPacketData({
                denom: denom,
                amount: amount,
                sender: Strings.toHexString(sender),
                receiver: receiver,
                memo: memo
            })
        );

        // ===== Case 4: Receiver chain is source, but denom is not erc20 address =====
        string memory invalidErc20Denom =
            string(abi.encodePacked(callbackMsg.payload.sourcePort, "/", sourceClient, "/", th.INVALID_ID()));
        callbackMsg.payload.value = abi.encode(
            IICS20TransferMsgs.FungibleTokenPacketData({
                denom: invalidErc20Denom,
                amount: amount,
                sender: Strings.toHexString(sender),
                receiver: receiver,
                memo: memo
            })
        );
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20DenomNotFound.selector, th.INVALID_ID()));
        vm.prank(ics26);
        ics20Transfer.onRecvPacket(callbackMsg);
        // reset denom
        callbackMsg.payload.value = abi.encode(
            IICS20TransferMsgs.FungibleTokenPacketData({
                denom: denom,
                amount: amount,
                sender: Strings.toHexString(sender),
                receiver: receiver,
                memo: memo
            })
        );

        // ===== Case 5: Invalid Receiver =====
        callbackMsg.payload.value = abi.encode(
            IICS20TransferMsgs.FungibleTokenPacketData({
                denom: denom,
                amount: amount,
                sender: Strings.toHexString(sender),
                receiver: th.INVALID_ID(),
                memo: memo
            })
        );
        vm.expectRevert(abi.encodeWithSelector(IICS20Errors.ICS20InvalidAddress.selector, th.INVALID_ID()));
        vm.prank(ics26);
        ics20Transfer.onRecvPacket(callbackMsg);
        // reset receiver
        callbackMsg.payload.value = abi.encode(
            IICS20TransferMsgs.FungibleTokenPacketData({
                denom: denom,
                amount: amount,
                sender: Strings.toHexString(sender),
                receiver: receiver,
                memo: memo
            })
        );

        // ===== Case 6: Invalid Source Port =====
        callbackMsg.payload.sourcePort = th.INVALID_ID();
        vm.expectRevert(
            abi.encodeWithSelector(IICS20Errors.ICS20InvalidPort.selector, ICS20Lib.DEFAULT_PORT_ID, th.INVALID_ID())
        );
        vm.prank(ics26);
        ics20Transfer.onRecvPacket(callbackMsg);
        // reset source port
        callbackMsg.payload.sourcePort = ICS20Lib.DEFAULT_PORT_ID;

        // ===== Case 7: Invalid Dest Port =====
        callbackMsg.payload.destPort = th.INVALID_ID();
        vm.expectRevert(
            abi.encodeWithSelector(IICS20Errors.ICS20InvalidPort.selector, ICS20Lib.DEFAULT_PORT_ID, th.INVALID_ID())
        );
        vm.prank(ics26);
        ics20Transfer.onRecvPacket(callbackMsg);
        // reset dest port
        callbackMsg.payload.destPort = ICS20Lib.DEFAULT_PORT_ID;

        // ===== Case 8: Invalid Encoding =====
        callbackMsg.payload.encoding = th.INVALID_ID();
        vm.expectRevert(
            abi.encodeWithSelector(
                IICS20Errors.ICS20UnexpectedEncoding.selector, ICS20Lib.ICS20_ENCODING, th.INVALID_ID()
            )
        );
        vm.prank(ics26);
        ics20Transfer.onRecvPacket(callbackMsg);
        // reset encoding
        callbackMsg.payload.encoding = ICS20Lib.ICS20_ENCODING;
    }

    function _getEscrowMappingSlot(string memory clientId) internal pure returns (bytes32) {
        bytes32 ics20Slot = 0x823f7a8ea9ae6df0eb03ec5e1682d7a2839417ad8a91774118e6acf2e8d2f800;
        return keccak256(abi.encodePacked(clientId, ics20Slot));
    }
}
