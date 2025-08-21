// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

// solhint-disable custom-errors,max-line-length,no-inline-assembly,gas-small-strings

import { Test } from "forge-std/Test.sol";

import { IICS26RouterMsgs } from "../../contracts/msgs/IICS26RouterMsgs.sol";
import { IICS27GMPMsgs } from "../../contracts/msgs/IICS27GMPMsgs.sol";
import { IIBCAppCallbacks } from "../../contracts/msgs/IIBCAppCallbacks.sol";
import { IICS27Errors } from "../../contracts/errors/IICS27Errors.sol";

import { IICS26Router } from "../../contracts/interfaces/IICS26Router.sol";

import { ERC1967Proxy } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Proxy.sol";
import { ICS27Account } from "../../contracts/utils/ICS27Account.sol";
import { ICS27GMP } from "../../contracts/ICS27GMP.sol";
import { ICS27Lib } from "../../contracts/utils/ICS27Lib.sol";
import { AccessManager } from "@openzeppelin-contracts/access/manager/AccessManager.sol";
import { UpgradeableBeacon } from "@openzeppelin-contracts/proxy/beacon/UpgradeableBeacon.sol";
import { TestHelper } from "./utils/TestHelper.sol";
import { IntegrationEnv } from "./utils/IntegrationEnv.sol";
import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";

contract ICS27GMPTest is Test {
    ICS27GMP public ics27Gmp;
    AccessManager public accessManager;

    TestHelper public th = new TestHelper();
    IntegrationEnv public integrationEnv = new IntegrationEnv();

    address public mockIcs26 = makeAddr("mockIcs26");

    function setUp() public {
        address ics27AccountLogic = address(new ICS27Account());
        address ics27GmpLogic = address(new ICS27GMP());

        accessManager = new AccessManager(address(this));
        ERC1967Proxy proxy = new ERC1967Proxy(
            ics27GmpLogic, abi.encodeCall(ICS27GMP.initialize, (mockIcs26, ics27AccountLogic, address(accessManager)))
        );
        ics27Gmp = ICS27GMP(address(proxy));

        assertEq(address(ics27Gmp.ics26()), mockIcs26, "ICS26 address mismatch");
        address accountBeacon = ics27Gmp.getAccountBeacon();

        address implementation = UpgradeableBeacon(accountBeacon).implementation();
        assertEq(implementation, ics27AccountLogic, "Account beacon implementation mismatch");
    }

    function testFuzz_success_sendCall(uint16 saltLen, uint16 payloadLen, uint64 seq) public {
        vm.assume(seq > 0);

        address sender = makeAddr("sender");
        bytes memory salt = vm.randomBytes(saltLen);
        string memory receiver = th.randomString();
        string memory memo = th.randomString();
        bytes memory payload = vm.randomBytes(payloadLen);

        bytes memory expCall = abi.encodeCall(
            IICS26Router.sendPacket,
            (
                IICS26RouterMsgs.MsgSendPacket({
                    sourceClient: th.FIRST_CLIENT_ID(),
                    timeoutTimestamp: th.DEFAULT_TIMEOUT_TIMESTAMP(),
                    payload: IICS26RouterMsgs.Payload({
                        sourcePort: ICS27Lib.DEFAULT_PORT_ID,
                        destPort: ICS27Lib.DEFAULT_PORT_ID,
                        version: ICS27Lib.ICS27_VERSION,
                        encoding: ICS27Lib.ICS27_ENCODING,
                        value: abi.encode(
                            IICS27GMPMsgs.GMPPacketData({
                                sender: Strings.toHexString(sender),
                                receiver: receiver,
                                salt: salt,
                                payload: payload,
                                memo: memo
                            })
                        )
                    })
                })
            )
        );

        vm.mockCall(mockIcs26, expCall, abi.encode(seq));
        vm.expectCall(mockIcs26, expCall);

        vm.startPrank(sender);
        uint64 sequence = ics27Gmp.sendCall(
            IICS27GMPMsgs.SendCallMsg({
                receiver: receiver,
                payload: payload,
                salt: salt,
                memo: memo,
                timeoutTimestamp: th.DEFAULT_TIMEOUT_TIMESTAMP(),
                sourceClient: th.FIRST_CLIENT_ID()
            })
        );
        vm.stopPrank();

        assertEq(sequence, seq, "Sequence mismatch");
    }

    function testFuzz_success_onRecvPacket(uint16 saltLen, uint16 payloadLen, uint64 seq) public {
        vm.assume(seq > 0 && payloadLen > 0);

        address relayer = makeAddr("relayer");
        address receiver = makeAddr("receiver");
        bytes memory salt = vm.randomBytes(saltLen);
        string memory sender = th.randomString();
        string memory memo = th.randomString();
        bytes memory payload = vm.randomBytes(payloadLen);

        bytes memory mockResp = bytes("mockResp");
        bytes memory expAck = ICS27Lib.acknowledgement(mockResp);

        vm.mockCall(receiver, payload, mockResp);

        IIBCAppCallbacks.OnRecvPacketCallback memory msg_ = IIBCAppCallbacks.OnRecvPacketCallback({
            sourceClient: th.FIRST_CLIENT_ID(),
            destinationClient: th.SECOND_CLIENT_ID(),
            sequence: seq,
            payload: IICS26RouterMsgs.Payload({
                sourcePort: ICS27Lib.DEFAULT_PORT_ID,
                destPort: ICS27Lib.DEFAULT_PORT_ID,
                version: ICS27Lib.ICS27_VERSION,
                encoding: ICS27Lib.ICS27_ENCODING,
                value: abi.encode(
                    IICS27GMPMsgs.GMPPacketData({
                        sender: sender,
                        receiver: Strings.toHexString(receiver),
                        salt: salt,
                        payload: payload,
                        memo: memo
                    })
                )
            }),
            relayer: relayer
        });

        IICS27GMPMsgs.AccountIdentifier memory accountId =
            IICS27GMPMsgs.AccountIdentifier({ clientId: msg_.destinationClient, sender: sender, salt: salt });

        address predeterminedAccount = ics27Gmp.getOrComputeAccountAddress(accountId);
        assertTrue(predeterminedAccount != address(0), "Predetermined account address should not be zero");

        vm.expectCall(receiver, payload);
        vm.prank(mockIcs26);
        bytes memory ack = ics27Gmp.onRecvPacket(msg_);
        assertEq(ack, expAck, "Acknowledgement mismatch");

        address actualAccount = ics27Gmp.getOrComputeAccountAddress(accountId);
        assertEq(actualAccount, predeterminedAccount, "Account address mismatch");
    }

    function testFuzz_failure_onRecvPacket(uint16 saltLen, uint16 payloadLen, uint64 seq) public {
        vm.assume(seq > 0 && payloadLen > 0);

        address relayer = makeAddr("relayer");
        address receiver = makeAddr("receiver");
        bytes memory salt = vm.randomBytes(saltLen);
        string memory sender = th.randomString();

        bytes memory errPayload = bytes("errPayload");
        bytes memory mockErr = bytes("mockErr");

        bytes memory payload = vm.randomBytes(payloadLen);
        bytes memory mockResp = bytes("mockResp");

        vm.mockCallRevert(receiver, errPayload, mockErr);
        vm.mockCall(receiver, payload, mockResp);

        IIBCAppCallbacks.OnRecvPacketCallback memory msg_ = IIBCAppCallbacks.OnRecvPacketCallback({
            sourceClient: th.FIRST_CLIENT_ID(),
            destinationClient: th.SECOND_CLIENT_ID(),
            sequence: seq,
            payload: IICS26RouterMsgs.Payload({
                sourcePort: ICS27Lib.DEFAULT_PORT_ID,
                destPort: ICS27Lib.DEFAULT_PORT_ID,
                version: ICS27Lib.ICS27_VERSION,
                encoding: ICS27Lib.ICS27_ENCODING,
                value: abi.encode(
                    IICS27GMPMsgs.GMPPacketData({
                        sender: sender,
                        receiver: Strings.toHexString(receiver),
                        salt: salt,
                        payload: payload,
                        memo: ""
                    })
                )
            }),
            relayer: relayer
        });

        // ===== Case 1: Incorrect Source Port =====
        msg_.payload.sourcePort = th.INVALID_ID();
        vm.expectRevert(
            abi.encodeWithSelector(
                IICS27Errors.ICS27InvalidPort.selector, ICS27Lib.DEFAULT_PORT_ID, msg_.payload.sourcePort
            )
        );
        vm.prank(mockIcs26);
        ics27Gmp.onRecvPacket(msg_);
        msg_.payload.sourcePort = ICS27Lib.DEFAULT_PORT_ID;

        // ===== Case 2: Incorrect Dest Port =====
        msg_.payload.destPort = th.INVALID_ID();
        vm.expectRevert(
            abi.encodeWithSelector(
                IICS27Errors.ICS27InvalidPort.selector, ICS27Lib.DEFAULT_PORT_ID, msg_.payload.destPort
            )
        );
        vm.prank(mockIcs26);
        ics27Gmp.onRecvPacket(msg_);
        msg_.payload.destPort = ICS27Lib.DEFAULT_PORT_ID;

        // ===== Case 3: Incorrect Version =====
        msg_.payload.version = th.INVALID_ID();
        vm.expectRevert(
            abi.encodeWithSelector(
                IICS27Errors.ICS27UnexpectedVersion.selector, ICS27Lib.ICS27_VERSION, msg_.payload.version
            )
        );
        vm.prank(mockIcs26);
        ics27Gmp.onRecvPacket(msg_);
        msg_.payload.version = ICS27Lib.ICS27_VERSION;

        // ===== Case 4: Incorrect Encoding =====
        msg_.payload.encoding = th.INVALID_ID();
        vm.expectRevert(
            abi.encodeWithSelector(
                IICS27Errors.ICS27UnexpectedEncoding.selector, ICS27Lib.ICS27_ENCODING, msg_.payload.encoding
            )
        );
        vm.prank(mockIcs26);
        ics27Gmp.onRecvPacket(msg_);
        msg_.payload.encoding = ICS27Lib.ICS27_ENCODING;

        // ===== Case 5: Empty Payload =====
        msg_.payload.value = abi.encode(
            IICS27GMPMsgs.GMPPacketData({
                sender: sender,
                receiver: Strings.toHexString(receiver),
                salt: salt,
                payload: bytes(""),
                memo: ""
            })
        );
        vm.expectRevert(IICS27Errors.ICS27PayloadEmpty.selector);
        vm.prank(mockIcs26);
        ics27Gmp.onRecvPacket(msg_);

        // ===== Case 6: Call reverts with the mock error =====
        msg_.payload.value = abi.encode(
            IICS27GMPMsgs.GMPPacketData({
                sender: sender,
                receiver: Strings.toHexString(receiver),
                salt: salt,
                payload: errPayload,
                memo: ""
            })
        );
        vm.prank(mockIcs26);
        vm.expectRevert();
        ics27Gmp.onRecvPacket(msg_);

        // ===== Case 7: Invalid Receiver =====
        msg_.payload.value = abi.encode(
            IICS27GMPMsgs.GMPPacketData({
                sender: sender,
                receiver: th.INVALID_ID(),
                salt: salt,
                payload: payload,
                memo: ""
            })
        );
        vm.expectRevert(abi.encodeWithSelector(IICS27Errors.ICS27InvalidReceiver.selector, th.INVALID_ID()));
        vm.prank(mockIcs26);
        ics27Gmp.onRecvPacket(msg_);
    }
}
