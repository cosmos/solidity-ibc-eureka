// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

// solhint-disable custom-errors,max-line-length

import { Test } from "forge-std/Test.sol";
import { IICS02ClientMsgs } from "../../contracts/msgs/IICS02ClientMsgs.sol";
import { IICS02ClientMsgs } from "../../contracts/msgs/IICS02ClientMsgs.sol";
import { ICS26Router } from "../../contracts/ICS26Router.sol";
import { ICS02Client } from "../../contracts/ICS02Client.sol";
import { IICS26Router } from "../../contracts/interfaces/IICS26Router.sol";
import { IICS26RouterMsgs } from "../../contracts/msgs/IICS26RouterMsgs.sol";
import { ICS20Transfer } from "../../contracts/ICS20Transfer.sol";
import { ICS20Lib } from "../../contracts/utils/ICS20Lib.sol";
import { Strings } from "@openzeppelin/utils/Strings.sol";
import { DummyLightClient } from "./mocks/DummyLightClient.sol";
import { ILightClientMsgs } from "../../contracts/msgs/ILightClientMsgs.sol";
import { TransparentUpgradeableProxy } from "@openzeppelin/proxy/transparent/TransparentUpgradeableProxy.sol";

contract ICS26RouterTest is Test {
    ICS26Router public ics26Router;

    bytes[] public merklePrefix = [bytes("ibc"), bytes("")];

    function setUp() public {
        ICS02Client ics02ClientLogic = new ICS02Client();
        ICS26Router ics26RouterLogic = new ICS26Router();

        TransparentUpgradeableProxy coreProxy = new TransparentUpgradeableProxy(
            address(ics02ClientLogic), address(this), abi.encodeWithSelector(ICS02Client.initialize.selector, address(this))
        );
        TransparentUpgradeableProxy routerProxy = new TransparentUpgradeableProxy(
            address(ics26RouterLogic),
            address(this),
            abi.encodeWithSelector(ICS26Router.initialize.selector, address(this), address(coreProxy))
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

    function test_RecvPacketWithFailedMembershipVerification() public {
        string memory counterpartyID = "42-dummy-01";
        DummyLightClient lightClient = new DummyLightClient(ILightClientMsgs.UpdateResult.Update, 0, true);
        string memory clientIdentifier = ics26Router.ICS02_CLIENT().addClient(
            "07-tendermint", IICS02ClientMsgs.CounterpartyInfo(counterpartyID, merklePrefix), address(lightClient)
        );

        ICS20Transfer ics20TransferLogic = new ICS20Transfer();
        TransparentUpgradeableProxy transferProxy = new TransparentUpgradeableProxy(
            address(ics20TransferLogic),
            address(this),
            abi.encodeWithSelector(ICS20Transfer.initialize.selector, address(ics26Router))
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
            sourceChannel: counterpartyID,
            destChannel: clientIdentifier,
            timeoutTimestamp: uint64(block.timestamp + 1000),
            payloads: payloads
        });

        IICS26RouterMsgs.MsgRecvPacket memory msgRecvPacket = IICS26RouterMsgs.MsgRecvPacket({
            packet: packet,
            proofCommitment: "0x", // doesn't matter
            proofHeight: IICS02ClientMsgs.Height({ revisionNumber: 0, revisionHeight: 0 }) // doesn't matter
         });

        vm.expectRevert();
        ics26Router.recvPacket(msgRecvPacket);
    }
}
