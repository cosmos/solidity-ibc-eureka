// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.25 <0.9.0;

// solhint-disable custom-errors,max-line-length

import { Test } from "forge-std/src/Test.sol";
import { IICS02Client } from "../src/interfaces/IICS02Client.sol";
import { IICS02ClientMsgs } from "../src/msgs/IICS02ClientMsgs.sol";
import { ILightClient } from "../src/interfaces/ILightClient.sol";
import { IIBCAppCallbacks } from "../src/msgs/IIBCAppCallbacks.sol";
import { ICS20Transfer } from "../src/apps/transfer/ICS20Transfer.sol";
import { TestERC20 } from "./TestERC20.sol";
import { ICS02Client } from "../src/ICS02Client.sol";
import { ICS26Router } from "../src/ICS26Router.sol";
import { IICS26RouterMsgs } from "../src/msgs/IICS26RouterMsgs.sol";
import { DummyLightClient } from "./DummyLightClient.sol";
import { ILightClientMsgs } from "../src/msgs/ILightClientMsgs.sol";
import { Ownable } from "@openzeppelin/contracts/access/Ownable.sol";
import { ICS20Lib } from "../src/apps/transfer/ICS20Lib.sol";
import { ICS24Host } from "../src/utils/ICS24Host.sol";

contract IntegrationTest is Test {
    IICS02Client public ics02Client;
    ICS26Router public ics26Router;
    ILightClient public lightClient;
    string public clientIdentifier;
    ICS20Transfer public ics20Transfer;
    string public ics20AddressStr;
    TestERC20 public erc20;
    string public counterpartyClient = "42-dummy-01";

    address public sender;

    function setUp() public {
        ics02Client = new ICS02Client(address(this));
        ics26Router = new ICS26Router(address(ics02Client), address(this));
        lightClient = new DummyLightClient(ILightClientMsgs.UpdateResult.Update, 0);
        ics20Transfer = new ICS20Transfer(address(ics26Router));
        erc20 = new TestERC20();

        clientIdentifier = ics02Client.addClient(
            "07-tendermint", IICS02ClientMsgs.CounterpartyInfo(counterpartyClient), address(lightClient)
        );
        ics20AddressStr = ICS20Lib.addressToHexString(address(ics20Transfer));
        ics26Router.addIBCApp("", address(ics20Transfer));

        sender = makeAddr("sender");
    }

    function test_success_sendICS20Packet() public {
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
            version: ics20Transfer.ICS20_VERSION()
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
            version: ics20Transfer.ICS20_VERSION(),
            data: data
        });
        assertEq(commitment, ICS24Host.packetCommitmentBytes32(expectedPacket));
    }

    function test_failure_sendICS20Packet() public {
        uint256 amount = 1000;
        erc20.mint(sender, amount);
        vm.startPrank(sender);
        erc20.approve(address(ics20Transfer), amount);

        string memory erc20AddressStr = ICS20Lib.addressToHexString(address(erc20));
        string memory senderStr = ICS20Lib.addressToHexString(sender);
        bytes memory data = ICS20Lib.marshalJSON(erc20AddressStr, amount, senderStr, "someReceiver", "memo");

        // sending from anywhere but the ics26router should fail
        IICS26RouterMsgs.Packet memory packet = IICS26RouterMsgs.Packet({
            sourcePort: ics20AddressStr,
            sourceChannel: clientIdentifier,
            destPort: "transfer",
            destChannel: counterpartyClient,
            data: data,
            timeoutTimestamp: uint32(block.timestamp) + 1000,
            sequence: 1,
            version: ics20Transfer.ICS20_VERSION()
        });
        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, sender));
        ics20Transfer.onSendPacket(IIBCAppCallbacks.OnSendPacketCallback({ packet: packet, sender: sender }));
    }
}
