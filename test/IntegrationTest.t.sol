// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.25 <0.9.0;

// solhint-disable custom-errors,max-line-length

import {Test} from "forge-std/src/Test.sol";
import "../src/interfaces/IICS02Client.sol";
import "../src/interfaces/IICS26Router.sol";
import "../src/interfaces/ILightClient.sol";
import "../src/apps/transfer/ICS20Transfer.sol";
import "./TestERC20.sol";
import "../src/ICS02Client.sol";
import "../src/ICS26Router.sol";
import {DummyLightClient} from "./DummyLightClient.sol";

contract IntegrationTest is Test {
    IICS02Client ics02Client;
    ICS26Router ics26Router;
    ILightClient lightClient;
    string clientIdentifier;
    ICS20Transfer ics20Transfer;
    string ics20AddressStr;
    TestERC20 erc20;
    string counterpartyClient = "42-dummy-01";

    address sender;

    function setUp() public {
        ics02Client = new ICS02Client(address(this));
        ics26Router = new ICS26Router(address(ics02Client), address(this));
        lightClient = new DummyLightClient(ILightClientMsgs.UpdateResult.Update, 0);
        ics20Transfer = new ICS20Transfer();
        erc20 = new TestERC20();

        clientIdentifier = ics02Client.addClient("07-tendermint", IICS02ClientMsgs.CounterpartyInfo(counterpartyClient), address(lightClient));
        ics20AddressStr = ICS20Lib.addressToHexString(address(ics20Transfer));
        ics26Router.addIBCApp("", address(ics20Transfer));

        sender = makeAddr("sender");
    }

    function test_sendICS20Packet() public {
        uint256 amount = 1000;
        erc20.mint(sender, amount);
        vm.startPrank(sender);
        erc20.approve(address(ics20Transfer), amount);

        string memory erc20AddressStr = ICS20Lib.addressToHexString(address(erc20));
        string memory senderStr = ICS20Lib.addressToHexString(sender);
        bytes memory data = ICS20Lib.marshalJSON(erc20AddressStr, amount, senderStr, "someReceiver", "memo");

        IICS26RouterMsgs.MsgSendPacket memory packet = IICS26RouterMsgs.MsgSendPacket({
            sourcePort: ics20AddressStr,
            sourceChannel: clientIdentifier,
            destPort: "transfer",
            data: data,
            timeoutTimestamp: uint32(block.timestamp) + 1000,
            version: "version"
        });
        uint32 sequence = ics26Router.sendPacket(packet);
        assertEq(sequence, 1);

        bytes32 path = ICS24Host.packetCommitmentKeyCalldata(packet.sourcePort, packet.sourceChannel, sequence);
        bytes32 commitment = ics26Router.getCommitment(path);
        IICS26RouterMsgs.Packet memory expectedPacket = IICS26RouterMsgs.Packet({
            sequence: sequence,
            timeoutTimestamp: uint32(block.timestamp) + 1000,
            sourcePort: ics20AddressStr,
            sourceChannel: clientIdentifier,
            destPort: "transfer",
            destChannel: counterpartyClient,
            version: "version",
            data: data
        });
        assertEq(commitment, ICS24Host.packetCommitmentBytes32(expectedPacket));
    }
}