//! Solana client state for IBC light client

use serde::{Deserialize, Serialize};
use solana_types::consensus::fork::ForkParameters;
use crate::error::SolanaIBCError;

/// Minimal Solana client state for IBC light client
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientState {
    /// Latest slot height for tracking progression
    pub latest_slot: u64,
    /// Fork parameters (minimal for now)
    pub fork_parameters: ForkParameters,
    /// Whether the client is frozen due to misbehavior
    pub is_frozen: bool,
}

impl ClientState {
    /// Verify fork support at a given epoch (simplified)
    pub fn verify_supported_fork_at_epoch(&self, _epoch: u64) -> Result<(), SolanaIBCError> {
        // For now, always accept any epoch since we have minimal fork handling
        Ok(())
    }

    /// Compute epoch at slot (simplified)
    pub fn compute_epoch_at_slot(&self, slot: u64) -> u64 {
        // Simplified: assume 432,000 slots per epoch (approximately 2 days at 400ms per slot)
        slot / 432_000
    }
}
