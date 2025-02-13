// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

// This is a helper to deploy the IBC implementation for testing purposes

import { Vm } from "forge-std/Vm.sol";
import { Test } from "forge-std/Test.sol";

import { ILightClientMsgs } from "../../../contracts/msgs/ILightClientMsgs.sol";
import { IICS02ClientMsgs } from "../../../contracts/msgs/IICS02ClientMsgs.sol";
import { IICS26RouterMsgs } from "../../../contracts/msgs/IICS26RouterMsgs.sol";
import { IICS20TransferMsgs } from "../../../contracts/msgs/IICS20TransferMsgs.sol";

import { IERC20 } from "@openzeppelin-contracts/token/ERC20/IERC20.sol";
import { IICS26Router } from "../../../contracts/interfaces/IICS26Router.sol";
import { ISignatureTransfer } from "@uniswap/permit2/src/interfaces/ISignatureTransfer.sol";

import { ICS26Router } from "../../../contracts/ICS26Router.sol";
import { IBCERC20 } from "../../../contracts/utils/IBCERC20.sol";
import { Escrow } from "../../../contracts/utils/Escrow.sol";
import { ICS20Transfer } from "../../../contracts/ICS20Transfer.sol";
import { TestValues } from "./TestValues.sol";
import { SolidityLightClient } from "../utils/SolidityLightClient.sol";
import { ICS20Lib } from "../../../contracts/utils/ICS20Lib.sol";
import { ERC1967Proxy } from "@openzeppelin-contracts/proxy/ERC1967/ERC1967Proxy.sol";
import { ICS24Host } from "../../../contracts/utils/ICS24Host.sol";

contract IbcImpl is Test {
    ICS26Router public immutable ics26Router;
    ICS20Transfer public immutable ics20Transfer;

    TestValues private _testValues = new TestValues();

    constructor(address permit2) {
        // ============ Step 1: Deploy the logic contracts ==============
        address escrowLogic = address(new Escrow());
        address ibcERC20Logic = address(new IBCERC20());
        ICS26Router ics26RouterLogic = new ICS26Router();
        ICS20Transfer ics20TransferLogic = new ICS20Transfer();

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

    function sendTransferAsUser(IERC20 token, address sender, string calldata receiver, uint256 amount) external returns (IICS26RouterMsgs.Packet memory) {
        return sendTransferAsUser(token, sender, receiver, amount, _testValues.FIRST_CLIENT_ID());
    }

    function sendTransferAsUser(IERC20 token, address sender, string calldata receiver, uint256 amount, string memory sourceClient) public returns (IICS26RouterMsgs.Packet memory) {
        vm.startPrank(sender);
        token.approve(address(ics20Transfer), amount);
        vm.recordLogs();
        ics20Transfer.sendTransfer(IICS20TransferMsgs.SendTransferMsg({
            denom: address(token),
            amount: amount,
            receiver: receiver,
            sourceClient: sourceClient,
            destPort: ICS20Lib.DEFAULT_PORT_ID,
            timeoutTimestamp: uint64(block.timestamp + 10 minutes),
            memo: ""
        }));
        vm.stopPrank();

        return _getPacketFromSendEvent();
    }

    function sendTransferAsUser(IERC20 token, address sender, string calldata receiver, ISignatureTransfer.PermitTransferFrom memory permit, bytes memory signature) public returns (IICS26RouterMsgs.Packet memory) {
        return sendTransferAsUser(token, sender, receiver, _testValues.FIRST_CLIENT_ID(), permit, signature);
    }

    function sendTransferAsUser(IERC20 token, address sender, string calldata receiver, string memory sourceClient, ISignatureTransfer.PermitTransferFrom memory permit, bytes memory signature) public returns (IICS26RouterMsgs.Packet memory) {
        vm.startPrank(sender);
        vm.recordLogs();
        ics20Transfer.permitSendTransfer(IICS20TransferMsgs.SendTransferMsg({
            denom: address(token),
            amount: permit.permitted.amount,
            receiver: receiver,
            sourceClient: sourceClient,
            destPort: ICS20Lib.DEFAULT_PORT_ID,
            timeoutTimestamp: uint64(block.timestamp + 10 minutes),
            memo: ""
        }), permit, signature);
        vm.stopPrank();

        return _getPacketFromSendEvent();
    }

    function recvPacket(IICS26RouterMsgs.Packet calldata packet) external {
        IICS26RouterMsgs.MsgRecvPacket memory msgRecvPacket;
        msgRecvPacket.packet = packet;
        ics26Router.recvPacket(msgRecvPacket);
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

    function _getPacketFromSendEvent() private returns (IICS26RouterMsgs.Packet memory) {
        Vm.Log[] memory sendEvent = vm.getRecordedLogs();
        for (uint256 i = 0; i < sendEvent.length; i++) {
            Vm.Log memory log = sendEvent[i];
            for (uint256 j = 0; j < log.topics.length; j++) {
                if (log.topics[j] == IICS26Router.SendPacket.selector) {
                    return abi.decode(log.data, (IICS26RouterMsgs.Packet));
                }
            }
        }
        // solhint-disable-next-line gas-custom-errors
        revert("SendPacket event not found");
    }
}
