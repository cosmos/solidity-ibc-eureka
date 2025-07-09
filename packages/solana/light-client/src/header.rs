//! Solana header types for IBC light client

use serde::{Deserialize, Serialize};

/// Minimal Solana header for client updates
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Header {
    /// The slot this header is updating from (trusted slot)
    pub trusted_slot: u64,
    /// The new slot this header is updating to
    pub new_slot: u64,
    /// Timestamp of the new slot
    pub timestamp: u64,
    /// Signature data for verification (placeholder for now)
    pub signature_data: Vec<u8>,
}

/// Minimal sync committee structure (placeholder)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActiveSyncCommittee {
    /// Placeholder for sync committee data
    pub _placeholder: (),
}
