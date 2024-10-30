// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

// solhint-disable custom-errors,max-line-length

import { Test } from "forge-std/Test.sol";
import { IICS02ClientMsgs } from "../src/msgs/IICS02ClientMsgs.sol";
import { ICS26Router } from "../src/ICS26Router.sol";
import { IICS26Router } from "../src/interfaces/IICS26Router.sol";
import { IICS26RouterMsgs } from "../src/msgs/IICS26RouterMsgs.sol";
import { ICS20Transfer } from "../src/ICS20Transfer.sol";
import { Strings } from "@openzeppelin/utils/Strings.sol";
import { DummyLightClient } from "./mocks/DummyLightClient.sol";
import { ILightClientMsgs } from "../src/msgs/ILightClientMsgs.sol";

contract ICS26RouterTest is Test {
    ICS26Router public ics26Router;

    bytes[] public merklePrefix = [bytes("ibc"), bytes("")];

    function setUp() public {
        ics26Router = new ICS26Router(address(this));
    }

    function test_AddIBCAppUsingAddress() public {
        ICS20Transfer ics20Transfer = new ICS20Transfer(address(ics26Router));
        string memory ics20AddressStr = Strings.toHexString(address(ics20Transfer));

        vm.expectEmit();
        emit IICS26Router.IBCAppAdded(ics20AddressStr, address(ics20Transfer));
        ics26Router.addIBCApp("", address(ics20Transfer));

        assertEq(address(ics20Transfer), address(ics26Router.getIBCApp(ics20AddressStr)));
    }

    function test_AddIBCAppUsingNamedPort() public {
        ICS20Transfer ics20Transfer = new ICS20Transfer(address(ics26Router));

        vm.expectEmit();
        emit IICS26Router.IBCAppAdded("transfer", address(ics20Transfer));
        ics26Router.addIBCApp("transfer", address(ics20Transfer));

        assertEq(address(ics20Transfer), address(ics26Router.getIBCApp("transfer")));
    }

    function test_RecvPacketWithFailedMembershipVerification() public {
        string memory counterpartyClientID = "42-dummy-01";
        DummyLightClient lightClient = new DummyLightClient(ILightClientMsgs.UpdateResult.Update, 0, true);
        string memory clientIdentifier = ics26Router.ICS02_CLIENT().addClient(
            "07-tendermint", IICS02ClientMsgs.CounterpartyInfo(counterpartyClientID, merklePrefix), address(lightClient)
        );

        ICS20Transfer ics20Transfer = new ICS20Transfer(address(ics26Router));
        ics26Router.addIBCApp("transfer", address(ics20Transfer));

        IICS26RouterMsgs.Packet memory packet = IICS26RouterMsgs.Packet({
            sequence: 1,
            timeoutTimestamp: uint64(block.timestamp + 1000),
            sourcePort: "transfer",
            sourceChannel: counterpartyClientID,
            destPort: "transfer",
            destChannel: clientIdentifier,
            version: "ics20-1",
            data: "0x"
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
