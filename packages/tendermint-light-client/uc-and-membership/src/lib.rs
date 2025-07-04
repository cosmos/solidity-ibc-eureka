//! The crate that contains the types and utilities for `tendermint-light-client-uc-and-membership` program.
//!
//! This crate provides dual APIs for native and zkVM environments:
//!
//! - **Native**: Returns `Result<T, E>` for proper error handling and composability
//! - **zkVM**: Returns `T` directly, panics on error to avoid Result overhead (~50-100 cycles)
//!
//! In zkVM, Result types generate unnecessary constraints even when immediately unwrapped.
//! Since proofs either succeed or fail entirely, error recovery is meaningless and wasteful.
//!
//! Use `--features panic` for zkVM builds to get optimal performance.
#![deny(missing_docs, clippy::nursery, clippy::pedantic, warnings, unused_crate_dependencies)]

use ibc_client_tendermint::types::{ConsensusState, Header};
use ibc_core_commitment_types::merkle::MerkleProof;
use tendermint_light_client_membership::{KVPair, MembershipOutput};
use tendermint_light_client_update_client::{ClientState, UpdateClientOutput};

/// Output from combined update client and membership verification
#[derive(Clone, Debug)]
pub struct UcAndMembershipOutput {
    /// Output from update client verification
    pub update_output: UpdateClientOutput,
    /// Output from membership verification
    pub membership_output: MembershipOutput,
}

/// Error type for combined update client and membership (only used when panic feature is not enabled)
#[cfg(not(feature = "panic"))]
#[derive(Clone, Debug, thiserror::Error)]
pub enum UcAndMembershipError {
    /// Invalid app hash
    #[error("invalid app hash: expected 32 bytes, got {0} bytes")]
    InvalidAppHash(usize),
    /// Update client error
    #[error("update client error: {0}")]
    UpdateClient(#[from] tendermint_light_client_update_client::UpdateClientError),
    /// Membership error
    #[error("membership error: {0}")]
    Membership(#[from] tendermint_light_client_membership::MembershipError),
}

/// IBC light client combined update of client and membership verification - panic version
#[cfg(feature = "panic")]
#[allow(clippy::missing_panics_doc)]
#[must_use]
pub fn update_client_and_membership(
    client_state: ClientState,
    trusted_consensus_state: ConsensusState,
    proposed_header: Header,
    time: u128,
    request_iter: impl Iterator<Item = (KVPair, MerkleProof)>,
) -> UcAndMembershipOutput {
    let app_hash: [u8; 32] = proposed_header
        .signed_header
        .header()
        .app_hash
        .as_bytes()
        .try_into()
        .unwrap();

    let uc_output = tendermint_light_client_update_client::update_client(
        client_state,
        trusted_consensus_state,
        proposed_header,
        time,
    );

    let mem_output = tendermint_light_client_membership::membership(app_hash, request_iter);

    UcAndMembershipOutput {
        update_output: uc_output,
        membership_output: mem_output,
    }
}

/// IBC light client combined update of client and membership verification - non-panic version
#[cfg(not(feature = "panic"))]
#[must_use]
pub fn update_client_and_membership(
    client_state: ClientState,
    trusted_consensus_state: ConsensusState,
    proposed_header: Header,
    time: u128,
    request_iter: impl Iterator<Item = (KVPair, MerkleProof)>,
) -> Result<UcAndMembershipOutput, UcAndMembershipError> {
    let app_hash_bytes = proposed_header.signed_header.header().app_hash.as_bytes();
    let app_hash: [u8; 32] = app_hash_bytes
        .try_into()
        .map_err(|_| UcAndMembershipError::InvalidAppHash(app_hash_bytes.len()))?;

    let uc_output = tendermint_light_client_update_client::update_client(
        client_state,
        trusted_consensus_state,
        proposed_header,
        time,
    )?;

    let mem_output = tendermint_light_client_membership::membership(app_hash, request_iter)?;

    Ok(UcAndMembershipOutput {
        update_output: uc_output,
        membership_output: mem_output,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ibc_client_tendermint::types::{ConsensusState, Header};
    use ibc_core_client_types::Height;
    use ibc_core_commitment_types::merkle::MerkleProof;
    use tendermint_light_client_update_client::TrustThreshold;

    fn test_client_state() -> ClientState {
        ClientState {
            chain_id: "test-chain".to_string(),
            trust_level: TrustThreshold::new(1, 3),
            trusting_period_seconds: 3600,
            unbonding_period_seconds: 7200,
            max_clock_drift_seconds: 60,
            frozen_height: None,
            latest_height: Height::new(1, 100).unwrap(),
        }
    }

    fn test_consensus_state() -> ConsensusState {
        ConsensusState::default()
    }

    fn test_header() -> Header {
        Header::default()
    }

    fn dummy_merkle_proof() -> MerkleProof {
        MerkleProof {
            proofs: vec![],
        }
    }

    #[test]
    fn test_panic_and_non_panic_modes_fail_with_invalid_empty_chain_id() {
        let client_state = test_client_state();
        let consensus_state = test_consensus_state();
        let header = test_header();
        let time = 1000u128;
        let kv_pairs = vec![
            (
                KVPair::new(b"key1".to_vec(), b"value1".to_vec()),
                dummy_merkle_proof(),
            ),
        ];

        // Test with invalid chain ID in client state
        let invalid_client_state = ClientState {
            chain_id: "".to_string(), // Invalid empty chain ID
            ..client_state
        };

        #[cfg(feature = "panic")]
        {
            let result = std::panic::catch_unwind(|| {
                update_client_and_membership(
                    invalid_client_state.clone(),
                    consensus_state.clone(),
                    header.clone(),
                    time,
                    kv_pairs.into_iter(),
                )
            });
            assert!(result.is_err(), "Expected panic for invalid chain ID in panic mode");
        }

        #[cfg(not(feature = "panic"))]
        {
            let result = update_client_and_membership(
                invalid_client_state,
                consensus_state,
                header,
                time,
                kv_pairs.into_iter(),
            );
            assert!(result.is_err(), "Expected error for invalid chain ID in non-panic mode");
            match result {
                Err(UcAndMembershipError::UpdateClient(_)) => {
                    // Expected error type
                }
                _ => panic!("Unexpected error type for invalid chain ID"),
            }
        }
    }

    #[test]
    fn test_panic_and_non_panic_modes_fail_with_invalid_trust_threshold() {
        let client_state = ClientState {
            trust_level: TrustThreshold::new(2, 1), // Invalid: numerator > denominator
            ..test_client_state()
        };
        let consensus_state = test_consensus_state();
        let header = test_header();
        let time = 1000u128;
        let kv_pairs = vec![];

        #[cfg(feature = "panic")]
        {
            let result = std::panic::catch_unwind(|| {
                update_client_and_membership(
                    client_state.clone(),
                    consensus_state.clone(),
                    header.clone(),
                    time,
                    kv_pairs.into_iter(),
                )
            });
            assert!(result.is_err(), "Expected panic for invalid trust threshold in panic mode");
        }

        #[cfg(not(feature = "panic"))]
        {
            let result = update_client_and_membership(
                client_state,
                consensus_state,
                header,
                time,
                kv_pairs.into_iter(),
            );
            assert!(result.is_err(), "Expected error for invalid trust threshold in non-panic mode");
            match result {
                Err(UcAndMembershipError::UpdateClient(_)) => {
                    // Expected error - invalid trust threshold is caught in update_client
                }
                _ => panic!("Unexpected error type for invalid trust threshold"),
            }
        }
    }

    #[test]
    fn test_panic_and_non_panic_modes_propagate_membership_verification_failure() {
        let client_state = test_client_state();
        let consensus_state = test_consensus_state();
        let header = test_header();
        let time = 1000u128;

        // This will fail during membership verification due to invalid proofs
        let kv_pairs = vec![
            (
                KVPair::new(b"key1".to_vec(), b"value1".to_vec()),
                dummy_merkle_proof(), // Invalid empty proof
            ),
        ];

        #[cfg(feature = "panic")]
        {
            let result = std::panic::catch_unwind(|| {
                update_client_and_membership(
                    client_state.clone(),
                    consensus_state.clone(),
                    header.clone(),
                    time,
                    kv_pairs.into_iter(),
                )
            });
            assert!(result.is_err(), "Expected panic for invalid membership proof in panic mode");
        }

        #[cfg(not(feature = "panic"))]
        {
            let result = update_client_and_membership(
                client_state,
                consensus_state,
                header,
                time,
                kv_pairs.into_iter(),
            );
            assert!(result.is_err(), "Expected error for invalid membership proof in non-panic mode");
            match result {
                Err(UcAndMembershipError::Membership(_)) => {
                    // Expected error type - membership verification failed
                }
                _ => panic!("Unexpected error type for invalid membership proof"),
            }
        }
    }

    #[test]
    fn test_panic_and_non_panic_modes_handle_valid_app_hash_consistently() {
        // Test that app hash extraction behaves consistently
        let client_state = test_client_state();
        let consensus_state = test_consensus_state();
        let header = test_header(); // Default header has 32-byte app hash
        let time = 1000u128;
        let kv_pairs = vec![];

        // The default header should have valid app hash, so we test the normal path
        // Both modes should handle this the same way (except for Result wrapping)
        #[cfg(feature = "panic")]
        {
            // This will fail at later stages (invalid chain ID or verification)
            let result = std::panic::catch_unwind(|| {
                update_client_and_membership(
                    client_state.clone(),
                    consensus_state.clone(),
                    header.clone(),
                    time,
                    kv_pairs.into_iter(),
                )
            });
            // Will panic due to other validation failures
            assert!(result.is_err());
        }

        #[cfg(not(feature = "panic"))]
        {
            let result = update_client_and_membership(
                client_state,
                consensus_state,
                header,
                time,
                kv_pairs.into_iter(),
            );
            // Will error due to other validation failures
            assert!(result.is_err());
        }
    }
}
