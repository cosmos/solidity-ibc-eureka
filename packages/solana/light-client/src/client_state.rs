//! Solana client state for IBC light client

use serde::{Deserialize, Serialize};
use solana_types::consensus::fork::ForkParameters;

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
