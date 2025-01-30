// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

// solhint-disable custom-errors,max-line-length

import { Test } from "forge-std/Test.sol";
import { IICS02ClientMsgs } from "../../contracts/msgs/IICS02ClientMsgs.sol";
import { IICS02ClientMsgs } from "../../contracts/msgs/IICS02ClientMsgs.sol";
import { ICS26Router } from "../../contracts/ICS26Router.sol";
import { IICS26Router } from "../../contracts/interfaces/IICS26Router.sol";
import { IICS26RouterMsgs } from "../../contracts/msgs/IICS26RouterMsgs.sol";
import { ICS20Transfer } from "../../contracts/ICS20Transfer.sol";
import { ICS20Lib } from "../../contracts/utils/ICS20Lib.sol";
import { Strings } from "@openzeppelin-contracts/utils/Strings.sol";
import { DummyLightClient } from "./mocks/DummyLightClient.sol";
import { ILightClientMsgs } from "../../contracts/msgs/ILightClientMsgs.sol";
import { ERC1967Proxy } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Proxy.sol";

contract ICS26RouterTest is Test {
    ICS26Router public ics26Router;

    bytes[] public merklePrefix = [bytes("ibc"), bytes("")];

    function setUp() public {
        ICS26Router ics26RouterLogic = new ICS26Router();

        ERC1967Proxy routerProxy = new ERC1967Proxy(
            address(ics26RouterLogic),
            abi.encodeWithSelector(ICS26Router.initialize.selector, address(this), address(this))
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
        string memory clientIdentifier = ics26Router.addClient(
            "07-tendermint", IICS02ClientMsgs.CounterpartyInfo(counterpartyID, merklePrefix), address(lightClient)
        );

        ICS20Transfer ics20TransferLogic = new ICS20Transfer();
        ERC1967Proxy transferProxy = new ERC1967Proxy(
            address(ics20TransferLogic), abi.encodeWithSelector(ICS20Transfer.initialize.selector, address(ics26Router))
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

        vm.expectRevert();
        ics26Router.recvPacket(msgRecvPacket);
    }
}
