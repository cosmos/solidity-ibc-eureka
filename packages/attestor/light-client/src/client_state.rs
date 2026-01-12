//! Attestor client state for IBC light client

use alloy_primitives::Address;
use serde::{Deserialize, Serialize};

/// Minimal attestor client state for IBC light client
/// Contains only the essential information needed for client management
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientState {
    /// Attestor Ethereum addresses (20-byte addresses recovered from signatures)
    pub attestor_addresses: Vec<Address>,
    /// Minimum required signatures
    pub min_required_sigs: u8,
    /// Latest height for tracking progression
    pub latest_height: u64,
    /// Whether the client is frozen due to misbehavior
    pub is_frozen: bool,
}

impl ClientState {
    /// Construct a new client state from a list of attestor addresses and quorum/height metadata.
    #[must_use]
    pub const fn new(
        attestor_addresses: Vec<Address>,
        min_required_sigs: u8,
        latest_height: u64,
    ) -> Self {
        Self {
            attestor_addresses,
            min_required_sigs,
            latest_height,
            is_frozen: false,
        }
    }
}
