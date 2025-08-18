//! Attestor client state for IBC light client

use serde::{Deserialize, Serialize};

/// Minimal attestor client state for IBC light client
/// Contains only the essential information needed for client management
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientState {
    /// Authorized attestor EVM addresses
    pub attestors: Vec<[u8; 20]>,
    /// Minimum required signatures
    pub min_required_sigs: u8,
    /// Latest height for tracking progression
    pub latest_height: u64,
    /// Whether the client is frozen due to misbehavior
    pub is_frozen: bool,
}
