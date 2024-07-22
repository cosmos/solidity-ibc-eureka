// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.25;

import { IIBCApp } from "./interfaces/IIBCApp.sol";
import { IICS26Router } from "./interfaces/IICS26Router.sol";
import { IICS02Client } from "./interfaces/IICS02Client.sol";
import { IBCStore } from "./utils/IBCStore.sol";
import { Ownable } from "@openzeppelin/contracts/access/Ownable.sol";

contract ICS26Router is IICS26Router, IBCStore, Ownable {
    mapping(string => IIBCApp) private apps;
    IICS02Client private ics02Client;

    constructor(address ics02Client_, address owner) Ownable(owner) {
        ics02Client = IICS02Client(ics02Client_);
    }

    // @notice Returns the address of the IBC application given the port identifier
    // @param portId The port identifier
    // @return The address of the IBC application contract
    function getIBCApp(string calldata portId) external view returns (IIBCApp) {
        return apps[portId];
    }

    // @notice Adds an IBC application to the router
    // @dev Only the admin can submit non-empty port identifiers
    // @param portId The port identifier
    // @param app The address of the IBC application contract
    function addIBCApp(string calldata portId, address app) external {
        // TODO: implement
    }

    // @notice Sends a packet
    // @param msg The message for sending packets
    // @return The sequence number of the packet
    function sendPacket(MsgSendPacket calldata msg_) external returns (uint32) {
        // TODO: implement
        // IIBCApp app = IIBCApp(apps[msg.sourcePort]);
        return 0;
    }

    // @notice Receives a packet
    // @param msg The message for receiving packets
    function recvPacket(MsgRecvPacket calldata msg_) external {
        // TODO: implement
        // IIBCApp app = IIBCApp(apps[msg.packet.destPort]);
    }

    // @notice Acknowledges a packet
    // @param msg The message for acknowledging packets
    function ackPacket(MsgAckPacket calldata msg_) external {
        // TODO: implement
        // IIBCApp app = IIBCApp(apps[msg.packet.sourcePort]);
    }

    // @notice Timeouts a packet
    // @param msg The message for timing out packets
    function timeoutPacket(MsgTimeoutPacket calldata msg_) external {
        // TODO: implement
        // IIBCApp app = IIBCApp(apps[msg.packet.sourcePort]);
    }
}
