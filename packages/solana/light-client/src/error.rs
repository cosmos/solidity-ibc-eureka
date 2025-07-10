//! Error types for Solana light client

use thiserror::Error;

/// Main error type for Solana IBC operations
#[derive(Error, Debug)]
pub enum SolanaIBCError {
    /// Invalid slot progression
    #[error("Invalid slot progression: current {current}, new {new}")]
    InvalidSlotProgression {
        /// Current slot
        current: u64,
        /// Proposed new slot
        new: u64,
    },

    /// Invalid signature verification
    #[error("Signature verification failed")]
    InvalidSignature,

    /// Invalid header format
    #[error("Invalid header format: {reason}")]
    InvalidHeader {
        /// Reason for error
        reason: String,
    },

    /// Client is frozen
    #[error("Client is frozen")]
    ClientFrozen,

    /// Fork verification failed
    #[error("Fork verification failed: {reason}")]
    ForkVerificationFailed {
        /// Reason for error
        reason: String,
    },

    /// Slot computation failed
    #[error("Slot computation failed")]
    SlotComputationFailed,

    /// Signature Data is missing
    #[error("Missing signature data")]
    MissingSignatureData,
}
