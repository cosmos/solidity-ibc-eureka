// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

/// @title ICS27 GMP Messages
/// @notice Interface defining ICS27 GMP Messages
interface IICS27GMPMsgs {
    /// @notice Message for sending a GMP packet
    /// @param sourceClient The source client identifier
    /// @param receiver The receiver address of the contract call
    /// @param salt The salt used to generate the caller account address
    /// @param payload The payload of the call
    /// @param timeoutTimestamp The absolute timeout timestamp in unix seconds
    /// @param memo Optional memo
    struct SendCallMsg {
        string sourceClient;
        string receiver;
        bytes salt;
        bytes payload;
        uint64 timeoutTimestamp;
        string memo;
    }

    /// @notice GMPPacketData is the IBC payload for the GMP application.
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

    /// @notice GMPAcknowledgement is the IBC acknowledgement of the GMP application.
    /// @param result The result of the call
    struct GMPAcknowledgement {
        bytes result;
    }

    /// @notice GMPSolanaPayload is the ABI-encoded payload for Solana GMP execution.
    /// @param packedAccounts Packed account entries (34 bytes each: pubkey + is_signer + is_writable)
    /// @param instructionData Raw instruction data for the target program
    /// @param prefundLamports Lamports to prefund the caller PDA
    struct GMPSolanaPayload {
        bytes packedAccounts;
        bytes instructionData;
        uint32 prefundLamports;
    }

    /// @notice AccountIdentifier is used to identify a ICS27 account.
    /// @dev The keccak256 hash of abi.encode(AccountIdentifier) is used as the create2 salt
    /// @param clientId The (local) client identifier
    /// @param sender The sender of the packet
    /// @param salt The salt of the packet
    struct AccountIdentifier {
        string clientId;
        string sender;
        bytes salt;
    }
}
