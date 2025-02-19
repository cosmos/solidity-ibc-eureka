// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

// solhint-disable custom-errors,max-line-length

import { Test } from "forge-std/Test.sol";

import { IICS02ClientMsgs } from "../../contracts/msgs/IICS02ClientMsgs.sol";
import { IICS02ClientMsgs } from "../../contracts/msgs/IICS02ClientMsgs.sol";
import { IICS26RouterMsgs } from "../../contracts/msgs/IICS26RouterMsgs.sol";
import { ILightClientMsgs } from "../../contracts/msgs/ILightClientMsgs.sol";
import { IIBCAppCallbacks } from "../../contracts/msgs/IIBCAppCallbacks.sol";

import { IICS26RouterErrors } from "../../contracts/errors/IICS26RouterErrors.sol";
import { IICS26Router } from "../../contracts/interfaces/IICS26Router.sol";

import { ICS26Router } from "../../contracts/ICS26Router.sol";
import { ICS20Transfer } from "../../contracts/ICS20Transfer.sol";
import { ICS20Lib } from "../../contracts/utils/ICS20Lib.sol";
import { ICS24Host } from "../../contracts/utils/ICS24Host.sol";
import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";
import { DummyLightClient } from "./mocks/DummyLightClient.sol";
import { ERC1967Proxy } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Proxy.sol";
import { IBCERC20 } from "../../contracts/utils/IBCERC20.sol";
import { Escrow } from "../../contracts/utils/Escrow.sol";

contract ICS26RouterTest is Test {
    ICS26Router public ics26Router;

    bytes[] public merklePrefix = [bytes("ibc"), bytes("")];

    function setUp() public {
        ICS26Router ics26RouterLogic = new ICS26Router();

        ERC1967Proxy routerProxy = new ERC1967Proxy(
            address(ics26RouterLogic), abi.encodeCall(ICS26Router.initialize, (address(this), address(this)))
        );

        ics26Router = ICS26Router(address(routerProxy));
    }

    function test_AddIBCAppUsingAddress() public {
        ICS20Transfer ics20Transfer = new ICS20Transfer();
        string memory ics20AddressStr = Strings.toHexString(address(ics20Transfer));

        vm.expectEmit();
        emit IICS26Router.IBCAppAdded(ics20AddressStr, address(ics20Transfer));
        ics26Router.addIBCApp("", address(ics20Transfer));

        assertEq(address(ics20Transfer), address(ics26Router.getIBCApp(ics20AddressStr)));
    }

    function test_AddIBCAppUsingNamedPort() public {
        ICS20Transfer ics20Transfer = new ICS20Transfer();

        vm.expectEmit();
        emit IICS26Router.IBCAppAdded(ICS20Lib.DEFAULT_PORT_ID, address(ics20Transfer));
        ics26Router.addIBCApp(ICS20Lib.DEFAULT_PORT_ID, address(ics20Transfer));

        assertEq(address(ics20Transfer), address(ics26Router.getIBCApp(ICS20Lib.DEFAULT_PORT_ID)));
    }

    function test_UnauthorizedSender() public {
        ICS20Transfer ics20Transfer = new ICS20Transfer();
        ics26Router.addIBCApp(ICS20Lib.DEFAULT_PORT_ID, address(ics20Transfer));

        address unauthorizedSender = makeAddr("unauthorizedSender");

        IICS26RouterMsgs.MsgSendPacket memory msgSendPacket;
        msgSendPacket.payload.sourcePort = ICS20Lib.DEFAULT_PORT_ID;

        vm.prank(unauthorizedSender);
        vm.expectRevert(abi.encodeWithSelector(IICS26RouterErrors.IBCUnauthorizedSender.selector, unauthorizedSender));
        ics26Router.sendPacket(msgSendPacket);
    }

    function test_RecvPacketWithFailedMembershipVerification() public {
        string memory counterpartyID = "42-dummy-01";
        DummyLightClient lightClient = new DummyLightClient(ILightClientMsgs.UpdateResult.Update, 0, true);
        string memory clientIdentifier =
            ics26Router.addClient(IICS02ClientMsgs.CounterpartyInfo(counterpartyID, merklePrefix), address(lightClient));

        ICS20Transfer ics20TransferLogic = new ICS20Transfer();
        address escrowLogic = address(new Escrow());
        address ibcERC20Logic = address(new IBCERC20());
        ERC1967Proxy transferProxy = new ERC1967Proxy(
            address(ics20TransferLogic),
            abi.encodeCall(
                ICS20Transfer.initialize, (address(ics26Router), escrowLogic, ibcERC20Logic, address(0), address(0))
            )
        );
        ICS20Transfer ics20Transfer = ICS20Transfer(address(transferProxy));
        ics26Router.addIBCApp(ICS20Lib.DEFAULT_PORT_ID, address(ics20Transfer));

        IICS26RouterMsgs.Payload[] memory payloads = new IICS26RouterMsgs.Payload[](1);
        payloads[0] = IICS26RouterMsgs.Payload({
            sourcePort: ICS20Lib.DEFAULT_PORT_ID,
            destPort: ICS20Lib.DEFAULT_PORT_ID,
            version: ICS20Lib.ICS20_VERSION,
            encoding: ICS20Lib.ICS20_ENCODING,
            value: "0x"
        });
        IICS26RouterMsgs.Packet memory packet = IICS26RouterMsgs.Packet({
            sequence: 1,
            sourceClient: counterpartyID,
            destClient: clientIdentifier,
            timeoutTimestamp: uint64(block.timestamp + 1000),
            payloads: payloads
        });

        IICS26RouterMsgs.MsgRecvPacket memory msgRecvPacket = IICS26RouterMsgs.MsgRecvPacket({
            packet: packet,
            proofCommitment: "0x", // doesn't matter
            proofHeight: IICS02ClientMsgs.Height({ revisionNumber: 0, revisionHeight: 0 }) // doesn't matter
         });

        vm.expectRevert(abi.encodeWithSelector(DummyLightClient.MembershipShouldFail.selector));
        ics26Router.recvPacket(msgRecvPacket);
    }

    function test_RecvPacketWithErrorAck() public {
        string memory counterpartyID = "42-dummy-01";
        DummyLightClient lightClient = new DummyLightClient(ILightClientMsgs.UpdateResult.Update, 0, false);
        string memory clientIdentifier =
            ics26Router.addClient(IICS02ClientMsgs.CounterpartyInfo(counterpartyID, merklePrefix), address(lightClient));

        // We add an unusable ICS20Transfer app to the router (not wrapped in a proxy)
        ICS20Transfer ics20Transfer = new ICS20Transfer();
        ics26Router.addIBCApp(ICS20Lib.DEFAULT_PORT_ID, address(ics20Transfer));

        IICS26RouterMsgs.Payload[] memory payloads = new IICS26RouterMsgs.Payload[](1);
        payloads[0] = IICS26RouterMsgs.Payload({
            sourcePort: ICS20Lib.DEFAULT_PORT_ID,
            destPort: ICS20Lib.DEFAULT_PORT_ID,
            version: ICS20Lib.ICS20_VERSION,
            encoding: ICS20Lib.ICS20_ENCODING,
            value: "0x"
        });
        IICS26RouterMsgs.Packet memory packet = IICS26RouterMsgs.Packet({
            sequence: 1,
            sourceClient: counterpartyID,
            destClient: clientIdentifier,
            timeoutTimestamp: uint64(block.timestamp + 1000),
            payloads: payloads
        });

        IICS26RouterMsgs.MsgRecvPacket memory msgRecvPacket = IICS26RouterMsgs.MsgRecvPacket({
            packet: packet,
            proofCommitment: "0x", // doesn't matter
            proofHeight: IICS02ClientMsgs.Height({ revisionNumber: 0, revisionHeight: 0 }) // doesn't matter
         });

        bytes[] memory expAcks = new bytes[](1);
        expAcks[0] = ICS24Host.UNIVERSAL_ERROR_ACK;

        vm.expectEmit();
        emit IICS26Router.WriteAcknowledgement(packet.destClient, packet.sequence, packet, expAcks);
        ics26Router.recvPacket(msgRecvPacket);
    }

    function test_RecvPacketWithOOG() public {
        string memory counterpartyID = "42-dummy-01";
        DummyLightClient lightClient = new DummyLightClient(ILightClientMsgs.UpdateResult.Update, 0, false);
        string memory clientIdentifier =
            ics26Router.addClient(IICS02ClientMsgs.CounterpartyInfo(counterpartyID, merklePrefix), address(lightClient));

        // We add an unusable ICS20Transfer app to the router (not wrapped in a proxy)
        MockApplication mockApp = new MockApplication();
        ics26Router.addIBCApp(ICS20Lib.DEFAULT_PORT_ID, address(mockApp));

        IICS26RouterMsgs.Payload[] memory payloads = new IICS26RouterMsgs.Payload[](1);
        payloads[0] = IICS26RouterMsgs.Payload({
            sourcePort: ICS20Lib.DEFAULT_PORT_ID,
            destPort: ICS20Lib.DEFAULT_PORT_ID,
            version: ICS20Lib.ICS20_VERSION,
            encoding: ICS20Lib.ICS20_ENCODING,
            value: "0x"
        });
        IICS26RouterMsgs.Packet memory packet = IICS26RouterMsgs.Packet({
            sequence: 1,
            sourceClient: counterpartyID,
            destClient: clientIdentifier,
            timeoutTimestamp: uint64(block.timestamp + 1000),
            payloads: payloads
        });

        IICS26RouterMsgs.MsgRecvPacket memory msgRecvPacket = IICS26RouterMsgs.MsgRecvPacket({
            packet: packet,
            proofCommitment: "0x", // doesn't matter
            proofHeight: IICS02ClientMsgs.Height({ revisionNumber: 0, revisionHeight: 0 }) // doesn't matter
         });

        vm.expectRevert(abi.encodeWithSelector(IICS26RouterErrors.IBCFailedCallback.selector));
        ics26Router.recvPacket{ gas: 900_000 }(msgRecvPacket);
    }
}

contract MockApplication is Test {
    function onRecvPacket(IIBCAppCallbacks.OnRecvPacketCallback calldata) external pure returns (bytes memory) {
        for (uint256 i = 0; i < 14_000; i++) {
            uint256 x;
            x = x * i;
        }

        return bytes("mock");
    }
}
