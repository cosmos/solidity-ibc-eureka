// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

interface IContractCallsMsgs {
    /// @notice ContractCallsPacketData is the payload for the contract call application.
    /// @param sender The sender of the packet
    /// @param receiver The receiver contract of the call
    /// @param payload The payload of the call
    /// @param memo Optional memo
    struct ContractCallsPacketData {
        string sender;
        string receiver;
        bytes payload;
        string memo;
    }
}
