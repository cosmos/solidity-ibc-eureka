//! Error types for attestor light client

use attestor_packet_membership::PacketAttestationError;
use k256::ecdsa::VerifyingKey;
use thiserror::Error;

/// Main error type for attestor IBC operations
#[derive(Error, Debug)]
pub enum IbcAttestorClientError {
    /// Invalid signature verification
    #[error("Signature verification failed")]
    InvalidSignature,

    /// Invalid header format
    #[error("Invalid header format: {reason}")]
    InvalidHeader {
        /// Reason for error
        reason: String,
    },

    /// Bad proof provided
    #[error("Proof invalid: {reason}")]
    InvalidProof {
        /// Reason for error
        reason: String,
    },

    /// Packet not found in attested data
    #[error("Membership proof failed: {0}")]
    MembershipProofFailed(#[from] PacketAttestationError),

    /// Unregistered public key (deprecated - use UnknownAddressRecovered)
    #[error("Unknown public key submitted {pubkey:?}")]
    UnknownPublicKeySubmitted {
        /// Bad key
        pubkey: VerifyingKey,
    },

    /// Unregistered address recovered from signature
    #[error("Unknown address recovered from signature: {address:02x?}")]
    UnknownAddressRecovered {
        /// Recovered address that is not in the trusted set
        address: [u8; 20],
    },

    /// Cannot attest to data as malformed
    #[error("Invalid attested data: {reason}")]
    InvalidAttestedData {
        /// Reason for error
        reason: String,
    },

    /// Proof cannot be deserialized
    #[error("deserializing membership proof failed: {0}")]
    DeserializeMembershipProofFailed(#[source] serde_json::Error),

    /// Client is frozen
    #[error("Client is frozen")]
    ClientFrozen,

    /// Height not found in consensus state
    #[error("Height {0} not found in consensus state")]
    HeightNotFound(u64),
}
