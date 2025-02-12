// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

// This is a helper to deploy the IBC implementation for testing purposes

import { Vm } from "forge-std/Vm.sol";
import { Test } from "forge-std/Test.sol";

import { ICS26Router } from "../../../contracts/ICS26Router.sol";
import { IBCERC20 } from "../../../contracts/utils/IBCERC20.sol";
import { Escrow } from "../../../contracts/utils/Escrow.sol";
import { ICS20Transfer } from "../../../contracts/ICS20Transfer.sol";

import { ILightClientMsgs } from "../../../contracts/msgs/ILightClientMsgs.sol";
import { IICS02ClientMsgs } from "../../../contracts/msgs/IICS02ClientMsgs.sol";
import { IICS26RouterMsgs } from "../../../contracts/msgs/IICS26RouterMsgs.sol";
import { IICS20TransferMsgs } from "../../../contracts/msgs/IICS20TransferMsgs.sol";

import { TestValues } from "./TestValues.sol";
import { SolidityLightClient } from "../utils/SolidityLightClient.sol";
import { ICS20Lib } from "../../../contracts/utils/ICS20Lib.sol";
import { ERC1967Proxy } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Proxy.sol";
import { ICS24Host } from "../../../contracts/utils/ICS24Host.sol";

contract IbcImpl is Test {
    ICS26Router public immutable ics26Router;
    ICS20Transfer public immutable ics20Transfer;

    TestValues private _testValues;

    constructor(address permit2) {
        // ============ Step 1: Deploy the logic contracts ==============
        address escrowLogic = address(new Escrow());
        address ibcERC20Logic = address(new IBCERC20());
        ICS26Router ics26RouterLogic = new ICS26Router();
        ICS20Transfer ics20TransferLogic = new ICS20Transfer();
        _testValues = new TestValues();

        // ============== Step 2: Deploy ERC1967 Proxies ==============
        ERC1967Proxy routerProxy = new ERC1967Proxy(
            address(ics26RouterLogic), abi.encodeCall(ICS26Router.initialize, (msg.sender, msg.sender))
        );

        ERC1967Proxy transferProxy = new ERC1967Proxy(
            address(ics20TransferLogic),
            abi.encodeCall(
                ICS20Transfer.initialize,
                (address(routerProxy), escrowLogic, ibcERC20Logic, address(0), address(permit2))
            )
        );

        ics26Router = ICS26Router(address(routerProxy));
        ics20Transfer = ICS20Transfer(address(transferProxy));

        // ============== Step 3: Wire up the contracts ==============
        vm.prank(msg.sender);
        ics26Router.addIBCApp(ICS20Lib.DEFAULT_PORT_ID, address(ics20Transfer));
    }

    /// @notice Adds a counterparty implementation by creating a solidity light client
    /// @param counterparty The counterparty implementation
    /// @param counterpartyId The counterparty identifier
    function addCounterpartyImpl(IbcImpl counterparty, string calldata counterpartyId) public returns (string memory) {
        ICS26Router counterpartyIcs26 = counterparty.ics26Router();
        SolidityLightClient lightClient = new SolidityLightClient(counterpartyIcs26);

        return ics26Router.addClient(IICS02ClientMsgs.CounterpartyInfo(counterpartyId, _testValues.EMPTY_MERKLE_PREFIX()), address(lightClient));
    }

    function getMsgMembershipForRecv(IICS26RouterMsgs.Packet calldata packet) external pure returns (ILightClientMsgs.MsgMembership memory) {
        bytes memory path = ICS24Host.packetCommitmentPathCalldata(packet.sourceClient, packet.sequence);
        bytes32 value = ICS24Host.packetCommitmentBytes32(packet);

        ILightClientMsgs.MsgMembership memory msg_;
        msg_.value = abi.encodePacked(value);
        msg_.path[0] = path;

        return msg_;
    }

    function getMsgMembershipForAck(IICS26RouterMsgs.Packet calldata packet, bytes[] memory acks) external pure returns (ILightClientMsgs.MsgMembership memory) {
        bytes memory path = ICS24Host.packetAcknowledgementCommitmentPathCalldata(packet.destClient, packet.sequence);
        bytes32 value = ICS24Host.packetAcknowledgementCommitmentBytes32(acks);

        ILightClientMsgs.MsgMembership memory msg_;
        msg_.value = abi.encodePacked(value);
        msg_.path[0] = path;

        return msg_;
    }

    function getMsgMembershipForTimeout(IICS26RouterMsgs.Packet calldata packet) external pure returns (ILightClientMsgs.MsgMembership memory) {
        bytes memory path = ICS24Host.packetReceiptCommitmentPathCalldata(packet.destClient, packet.sequence);

        ILightClientMsgs.MsgMembership memory msg_;
        msg_.value = bytes("");
        msg_.path[0] = path;

        return msg_;
    }
}
