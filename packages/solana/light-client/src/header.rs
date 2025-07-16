//! Attestor header types for IBC light client

use serde::{Deserialize, Serialize};

/// Minimal attestor header for client updates
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Header {
    /// The height this header is updating from (trusted height)
    pub trusted_height: u64,
    /// The new height this header is updating to
    pub new_height: u64,
    /// Timestamp of the new height
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
