//! Attestor client state for IBC light client

use alloy_primitives::Address;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors that can occur when creating a client state
#[derive(Debug, Error)]
pub enum ClientStateError {
    /// Returned when min_required_sigs is zero
    #[error("min_required_sigs must be greater than 0")]
    ZeroMinRequiredSigs,
    /// Returned when there are fewer attestors than required signatures
    #[error("attestor_addresses length ({0}) must be >= min_required_sigs ({1})")]
    InsufficientAttestors(usize, u8),
}

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
    ///
    /// # Errors
    /// Returns an error if:
    /// - `min_required_sigs` is 0
    /// - `attestor_addresses.len()` is less than `min_required_sigs`
    pub fn new(
        attestor_addresses: Vec<Address>,
        min_required_sigs: u8,
        latest_height: u64,
    ) -> Result<Self, ClientStateError> {
        if min_required_sigs == 0 {
            return Err(ClientStateError::ZeroMinRequiredSigs);
        }
        if attestor_addresses.len() < usize::from(min_required_sigs) {
            return Err(ClientStateError::InsufficientAttestors(
                attestor_addresses.len(),
                min_required_sigs,
            ));
        }
        Ok(Self {
            attestor_addresses,
            min_required_sigs,
            latest_height,
            is_frozen: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_succeeds_with_valid_params() {
        let addresses = vec![Address::from([0x11; 20]), Address::from([0x22; 20])];
        let state = ClientState::new(addresses.clone(), 2, 100).unwrap();

        assert_eq!(state.attestor_addresses, addresses);
        assert_eq!(state.min_required_sigs, 2);
        assert_eq!(state.latest_height, 100);
        assert!(!state.is_frozen);
    }

    #[test]
    fn new_fails_with_zero_min_required_sigs() {
        let addresses = vec![Address::from([0x11; 20])];
        let result = ClientState::new(addresses, 0, 100);

        assert!(matches!(result, Err(ClientStateError::ZeroMinRequiredSigs)));
    }

    #[test]
    fn new_fails_with_insufficient_attestors() {
        let addresses = vec![Address::from([0x11; 20])];
        let result = ClientState::new(addresses, 2, 100);

        assert!(matches!(
            result,
            Err(ClientStateError::InsufficientAttestors(1, 2))
        ));
    }

    #[test]
    fn new_allows_more_attestors_than_required() {
        let addresses = vec![
            Address::from([0x11; 20]),
            Address::from([0x22; 20]),
            Address::from([0x33; 20]),
        ];
        let state = ClientState::new(addresses, 2, 100).unwrap();

        assert_eq!(state.min_required_sigs, 2);
        assert_eq!(state.attestor_addresses.len(), 3);
    }
}
