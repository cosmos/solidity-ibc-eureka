//! Error types for attestor light client

use thiserror::Error;

/// Main error type for attestor IBC operations
#[derive(Error, Debug)]
pub enum SolanaIBCError {
    /// Invalid height progression
    #[error("Invalid height progression: current {current}, new {new}")]
    InvalidHeightProgression {
        /// Current height
        current: u64,
        /// Proposed new height
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

    /// Height computation failed
    #[error("Height computation failed")]
    HeightComputationFailed,

    /// Height not found in consensus state
    #[error("Height {0} not found in consensus state")]
    HeightNotFound(u64),
}
