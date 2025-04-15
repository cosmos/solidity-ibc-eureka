// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

interface IICS27GMPMsgs {
    /// @notice GMPPacketData is the payload for the GMP application.
    /// @param sender The sender of the packet
    /// @param receiver The receiver address of the contract call
    /// @param salt The salt used to generate the caller account address
    /// @param payload The payload of the call
    /// @param memo Optional memo
    struct GMPPacketData {
        string sender;
        string receiver;
        bytes salt;
        bytes payload;
        string memo;
    }
}
