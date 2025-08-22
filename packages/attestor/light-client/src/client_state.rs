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
    pub fn new(attestor_addresses: Vec<Address>, min_required_sigs: u8, latest_height: u64) -> Self {
        Self { attestor_addresses, min_required_sigs, latest_height, is_frozen: false }
    }
}

#[cfg(test)]
mod tests {
    use super::ClientState;
    use alloy_primitives::Address;

    #[test]
    fn client_state_new_from_addresses() {
        let addrs = vec![
            Address::from([0x11u8; 20]),
            Address::from([0x22u8; 20]),
        ];
        let client_state = ClientState::new(addrs.clone(), 2, 42);
        assert_eq!(client_state.attestor_addresses, addrs);
        assert_eq!(client_state.min_required_sigs, 2);
        assert_eq!(client_state.latest_height, 42);
        assert!(!client_state.is_frozen);
    }
}
