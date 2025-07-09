//! Solana consensus state for IBC light client

use serde::{Deserialize, Serialize};

/// Minimal Solana consensus state for IBC light client
/// Currently zero-size since consensus state is not used in verification
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsensusState {
    /// Slot number for IBC height tracking
    pub slot: u64,
    /// Timestamp for IBC queries
    pub timestamp: u64,
}
