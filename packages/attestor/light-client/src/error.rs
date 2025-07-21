//! Error types for attestor light client

use thiserror::Error;

/// Main error type for attestor IBC operations
#[derive(Error, Debug)]
pub enum SolanaIBCError {
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
