// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

/// @title IAttestorMsgs
/// @notice Message types for the attestor-based IBC light client
interface IAttestorMsgs {
    /// @notice Attested state for client updates
    /// @param height The new trusted height
    /// @param timestamp The timestamp (seconds) for the height
    struct StateAttestation {
        uint64 height;
        uint64 timestamp;
    }

    /// @notice Attested packet membership at a specific height
    /// @param height The height these packets correspond to
    /// @param packetCommitments The list of packet commitments attested as present at `height`
    struct PacketAttestation {
        uint64 height;
        bytes32[] packetCommitments;
    }

    /// @notice Generic proof payload used for both client updates and membership checks
    /// @dev attestationData is ABI-encoded payload:
    ///      - updateClient: abi.encode(StateAttestation)
    ///      - verifyMembership: abi.encode(PacketAttestation)
    /// @param attestationData ABI-encoded payload (see @dev)
    /// @param signatures Signatures over sha256(attestationData); each 65-byte (r||s||v)
    struct AttestationProof {
        bytes attestationData;
        bytes[] signatures;
    }
}
