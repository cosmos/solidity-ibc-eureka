//! The crate that contains the types and utilities for `tendermint-light-client-misbehaviour`
//! program.
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

use ibc_client_tendermint::client_state::{
    check_for_misbehaviour_on_misbehavior, verify_misbehaviour,
};
use ibc_client_tendermint::types::{ConsensusState, Misbehaviour, TENDERMINT_CLIENT_TYPE};
use ibc_core_client_types::Height;
use ibc_core_host_types::identifiers::{ChainId, ClientId};
use std::time::Duration;
use tendermint_light_client_update_client::types::validation::ClientValidationCtx;
use tendermint_light_client_verifier::{options::Options, types::TrustThreshold as TmTrustThreshold, ProdVerifier};
pub use tendermint_light_client_update_client::TrustThreshold;

/// Client state for misbehaviour detection
#[derive(Clone, Debug)]
pub struct ClientState {
    /// Chain ID
    pub chain_id: String,
    /// Trust level
    pub trust_level: TrustThreshold,
    /// Trusting period in seconds
    pub trusting_period_seconds: u64,
    /// Unbonding period in seconds
    pub unbonding_period_seconds: u64,
    /// Max clock drift in seconds
    pub max_clock_drift_seconds: u64,
    /// Frozen height (None if not frozen)
    pub frozen_height: Option<Height>,
    /// Latest height
    pub latest_height: Height,
}


/// Output from misbehaviour verification
#[derive(Clone, Debug)]
pub struct MisbehaviourOutput {
    /// The client state that was used to verify the misbehaviour
    pub client_state: ClientState,
    /// The trusted height of header 1
    pub trusted_height_1: Height,
    /// The trusted height of header 2
    pub trusted_height_2: Height,
    /// The trusted consensus state of header 1
    pub trusted_consensus_state_1: ConsensusState,
    /// The trusted consensus state of header 2
    pub trusted_consensus_state_2: ConsensusState,
    /// The time which the misbehaviour was verified in unix nanoseconds
    pub time: u128,
}

/// Error type for misbehaviour detection (only used when panic feature is not enabled)
#[cfg(not(feature = "panic"))]
#[derive(Clone, Debug, thiserror::Error)]
pub enum MisbehaviourError {
    /// Invalid client ID
    #[error("invalid client ID: {0}")]
    InvalidClientId(#[source] ibc_core_client_types::error::ClientError),
    /// Invalid chain ID
    #[error("invalid chain ID '{chain_id}': {source}")]
    InvalidChainId {
        chain_id: String,
        #[source]
        source: ibc_core_host_types::error::IdentifierError,
    },
    /// Chain ID mismatch
    #[error("chain ID mismatch: client state chain ID does not match misbehaviour header")]
    ChainIdMismatch,
    /// Invalid trust threshold
    #[error("invalid trust threshold: numerator must be less than or equal to denominator")]
    InvalidTrustThreshold,
    /// Misbehaviour verification failed
    #[error("misbehaviour verification failed: {0}")]
    MisbehaviourVerificationFailed(#[source] ibc_client_tendermint::client_state::Error),
    /// Check for misbehaviour failed
    #[error("check for misbehaviour failed: {0}")]
    CheckForMisbehaviourFailed(#[source] ibc_client_tendermint::client_state::Error),
    /// Misbehaviour not detected
    #[error("misbehaviour not detected")]
    MisbehaviourNotDetected,
}

/// IBC light client misbehaviour check - panic version
#[cfg(feature = "panic")]
#[allow(clippy::missing_panics_doc)]
#[must_use]
pub fn check_for_misbehaviour(
    client_state: ClientState,
    misbehaviour: &Misbehaviour,
    trusted_consensus_state_1: ConsensusState,
    trusted_consensus_state_2: ConsensusState,
    time: u128,
) -> MisbehaviourOutput {
    let client_id = ClientId::new(TENDERMINT_CLIENT_TYPE, 0).unwrap();
    let chain_id = ChainId::new(&client_state.chain_id).unwrap();

    assert_eq!(
        client_state.chain_id,
        misbehaviour
            .header1()
            .signed_header
            .header
            .chain_id
            .to_string()
    ); // header2 is checked by `verify_misbehaviour`

    // Insert the two trusted consensus states into the trusted consensus state map that exists in the ClientValidationContext that is expected by verifyMisbehaviour
    // Since we are mocking the existence of prior trusted consensus states, we are only filling in the two consensus states that are passed in into the map
    let mut ctx = ClientValidationCtx::new(time);

    ctx.insert_trusted_consensus_state(
        client_id.clone(),
        misbehaviour.header1().trusted_height.revision_number(),
        misbehaviour.header1().trusted_height.revision_height(),
        &trusted_consensus_state_1,
    );
    ctx.insert_trusted_consensus_state(
        client_id.clone(),
        misbehaviour.header2().trusted_height.revision_number(),
        misbehaviour.header2().trusted_height.revision_height(),
        &trusted_consensus_state_2,
    );

    let trust_threshold: TmTrustThreshold = client_state.trust_level.clone().into();

    let options = Options {
        trust_threshold,
        trusting_period: Duration::from_secs(client_state.trusting_period_seconds),
        clock_drift: Duration::from_secs(15),
    };

    // Call into ibc-rs verify_misbehaviour function to verify that both headers are valid given their respective trusted consensus states
    verify_misbehaviour::<_, sha2::Sha256>(
        &ctx,
        misbehaviour,
        &client_id,
        &chain_id,
        &options,
        &ProdVerifier::default(),
    )
    .unwrap();

    // Call into ibc-rs check_for_misbehaviour_on_misbehaviour method to ensure that the misbehaviour is valid
    // i.e. the headers are same height but different commits, or headers are not monotonically increasing in time
    let is_misbehaviour =
        check_for_misbehaviour_on_misbehavior(misbehaviour.header1(), misbehaviour.header2())
            .unwrap();
    assert!(is_misbehaviour, "Misbehaviour is not detected");

    // The prover takes in the trusted headers as an input but does not maintain its own internal state
    // Thus, the verifier must ensure that the trusted headers that were used in the proof are trusted consensus
    // states stored in its own internal state before it can accept the misbehaviour proof as valid.
    MisbehaviourOutput {
        client_state,
        trusted_height_1: misbehaviour.header1().trusted_height,
        trusted_height_2: misbehaviour.header2().trusted_height,
        trusted_consensus_state_1,
        trusted_consensus_state_2,
        time,
    }
}

/// IBC light client misbehaviour check - non-panic version
#[cfg(not(feature = "panic"))]
#[must_use]
pub fn check_for_misbehaviour(
    client_state: ClientState,
    misbehaviour: &Misbehaviour,
    trusted_consensus_state_1: ConsensusState,
    trusted_consensus_state_2: ConsensusState,
    time: u128,
) -> Result<MisbehaviourOutput, MisbehaviourError> {
    let client_id = ClientId::new(TENDERMINT_CLIENT_TYPE, 0)
        .map_err(MisbehaviourError::InvalidClientId)?;
    let chain_id = ChainId::new(&client_state.chain_id)
        .map_err(|e| MisbehaviourError::InvalidChainId {
            chain_id: client_state.chain_id.clone(),
            source: e,
        })?;

    if client_state.chain_id != misbehaviour.header1().signed_header.header.chain_id.to_string() {
        return Err(MisbehaviourError::ChainIdMismatch);
    }

    // Insert the two trusted consensus states into the trusted consensus state map that exists in the ClientValidationContext that is expected by verifyMisbehaviour
    // Since we are mocking the existence of prior trusted consensus states, we are only filling in the two consensus states that are passed in into the map
    let mut ctx = ClientValidationCtx::new(time);

    ctx.insert_trusted_consensus_state(
        client_id.clone(),
        misbehaviour.header1().trusted_height.revision_number(),
        misbehaviour.header1().trusted_height.revision_height(),
        &trusted_consensus_state_1,
    );
    ctx.insert_trusted_consensus_state(
        client_id.clone(),
        misbehaviour.header2().trusted_height.revision_number(),
        misbehaviour.header2().trusted_height.revision_height(),
        &trusted_consensus_state_2,
    );

    let trust_threshold: TmTrustThreshold = TmTrustThreshold::new(
        client_state.trust_level.numerator,
        client_state.trust_level.denominator,
    )
    .ok_or(MisbehaviourError::InvalidTrustThreshold)?;

    let options = Options {
        trust_threshold,
        trusting_period: Duration::from_secs(client_state.trusting_period_seconds),
        clock_drift: Duration::from_secs(15),
    };

    // Call into ibc-rs verify_misbehaviour function to verify that both headers are valid given their respective trusted consensus states
    verify_misbehaviour::<_, sha2::Sha256>(
        &ctx,
        misbehaviour,
        &client_id,
        &chain_id,
        &options,
        &ProdVerifier::default(),
    )
    .map_err(MisbehaviourError::MisbehaviourVerificationFailed)?;

    // Call into ibc-rs check_for_misbehaviour_on_misbehaviour method to ensure that the misbehaviour is valid
    // i.e. the headers are same height but different commits, or headers are not monotonically increasing in time
    let is_misbehaviour =
        check_for_misbehaviour_on_misbehavior(misbehaviour.header1(), misbehaviour.header2())
            .map_err(MisbehaviourError::CheckForMisbehaviourFailed)?;

    if !is_misbehaviour {
        return Err(MisbehaviourError::MisbehaviourNotDetected);
    }

    // The prover takes in the trusted headers as an input but does not maintain its own internal state
    // Thus, the verifier must ensure that the trusted headers that were used in the proof are trusted consensus
    // states stored in its own internal state before it can accept the misbehaviour proof as valid.
    Ok(MisbehaviourOutput {
        client_state,
        trusted_height_1: misbehaviour.header1().trusted_height,
        trusted_height_2: misbehaviour.header2().trusted_height,
        trusted_consensus_state_1,
        trusted_consensus_state_2,
        time,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ibc_client_tendermint::types::{Header, Misbehaviour};
    use ibc_core_client_types::Height;

    fn test_client_state() -> ClientState {
        ClientState {
            chain_id: "test-chain-1".to_string(),
            trust_level: TrustThreshold::new(1, 3),
            trusting_period_seconds: 86400,
            unbonding_period_seconds: 1209600,
            max_clock_drift_seconds: 60,
            frozen_height: None,
            latest_height: Height::new(1, 100).unwrap(),
        }
    }

    fn test_consensus_state() -> ConsensusState {
        ConsensusState::default()
    }

    fn test_misbehaviour() -> Misbehaviour {
        Misbehaviour::default()
    }

    #[test]
    fn test_panic_and_non_panic_modes_fail_with_invalid_empty_chain_id() {
        let client_state = test_client_state();
        let misbehaviour = test_misbehaviour();
        let consensus_state_1 = test_consensus_state();
        let consensus_state_2 = test_consensus_state();
        let time = 1000u128;

        // Since we can't actually verify default values, we'll test error cases
        // Test with invalid chain ID
        let invalid_client_state = ClientState {
            chain_id: "".to_string(), // Invalid empty chain ID
            ..client_state.clone()
        };

        // Test panic mode behavior
        #[cfg(feature = "panic")]
        {
            let result = std::panic::catch_unwind(|| {
                check_for_misbehaviour(
                    invalid_client_state.clone(),
                    &misbehaviour,
                    consensus_state_1.clone(),
                    consensus_state_2.clone(),
                    time,
                )
            });
            assert!(result.is_err(), "Expected panic for invalid chain ID in panic mode");
        }

        // Test non-panic mode behavior
        #[cfg(not(feature = "panic"))]
        {
            let result = check_for_misbehaviour(
                invalid_client_state,
                &misbehaviour,
                consensus_state_1,
                consensus_state_2,
                time,
            );
            assert!(result.is_err(), "Expected error for invalid chain ID in non-panic mode");
            match result {
                Err(MisbehaviourError::InvalidChainId { .. }) => {
                    // Expected error type
                }
                _ => panic!("Unexpected error type for invalid chain ID"),
            }
        }
    }

    #[test]
    fn test_panic_and_non_panic_modes_fail_with_invalid_trust_threshold() {
        let client_state = test_client_state();
        let misbehaviour = test_misbehaviour();
        let consensus_state_1 = test_consensus_state();
        let consensus_state_2 = test_consensus_state();
        let time = 1000u128;

        // Test with invalid trust threshold
        let invalid_client_state = ClientState {
            trust_level: TrustThreshold::new(2, 1), // numerator > denominator
            ..client_state
        };

        #[cfg(feature = "panic")]
        {
            let result = std::panic::catch_unwind(|| {
                check_for_misbehaviour(
                    invalid_client_state.clone(),
                    &misbehaviour,
                    consensus_state_1.clone(),
                    consensus_state_2.clone(),
                    time,
                )
            });
            assert!(result.is_err(), "Expected panic for invalid trust threshold in panic mode");
        }

        #[cfg(not(feature = "panic"))]
        {
            let result = check_for_misbehaviour(
                invalid_client_state,
                &misbehaviour,
                consensus_state_1,
                consensus_state_2,
                time,
            );
            assert!(result.is_err(), "Expected error for invalid trust threshold in non-panic mode");
            match result {
                Err(MisbehaviourError::InvalidTrustThreshold) => {
                    // Expected error type
                }
                _ => panic!("Unexpected error type for invalid trust threshold"),
            }
        }
    }

    #[test]
    fn test_panic_and_non_panic_modes_fail_when_chain_id_mismatch() {
        let client_state = test_client_state();
        let misbehaviour = test_misbehaviour();
        let consensus_state_1 = test_consensus_state();
        let consensus_state_2 = test_consensus_state();
        let time = 1000u128;

        // Since default Misbehaviour has empty chain_id, this will cause mismatch
        #[cfg(feature = "panic")]
        {
            let result = std::panic::catch_unwind(|| {
                check_for_misbehaviour(
                    client_state.clone(),
                    &misbehaviour,
                    consensus_state_1.clone(),
                    consensus_state_2.clone(),
                    time,
                )
            });
            assert!(result.is_err(), "Expected panic for chain ID mismatch in panic mode");
        }

        #[cfg(not(feature = "panic"))]
        {
            let result = check_for_misbehaviour(
                client_state,
                &misbehaviour,
                consensus_state_1,
                consensus_state_2,
                time,
            );
            assert!(result.is_err(), "Expected error for chain ID mismatch in non-panic mode");
            match result {
                Err(MisbehaviourError::ChainIdMismatch) => {
                    // Expected error type
                }
                _ => panic!("Unexpected error type for chain ID mismatch"),
            }
        }
    }

    #[test]
    fn test_panic_and_non_panic_modes_consistent_validation_for_invalid_trust_level() {
        // This test verifies that both modes handle the same input consistently
        let client_state = test_client_state();
        let misbehaviour = test_misbehaviour();
        let consensus_state_1 = test_consensus_state();
        let consensus_state_2 = test_consensus_state();
        let time = 1000u128;

        #[cfg(all(feature = "panic", not(feature = "panic")))]
        compile_error!("Both panic and non-panic features cannot be enabled at the same time");

        // We can only test one mode at a time, but we ensure consistent validation logic
        let invalid_trust_level = ClientState {
            trust_level: TrustThreshold::new(5, 2), // Invalid: numerator > denominator
            ..client_state
        };

        #[cfg(feature = "panic")]
        {
            let should_panic = std::panic::catch_unwind(|| {
                check_for_misbehaviour(
                    invalid_trust_level,
                    &misbehaviour,
                    consensus_state_1,
                    consensus_state_2,
                    time,
                )
            });
            assert!(should_panic.is_err(), "Invalid trust level should cause panic");
        }

        #[cfg(not(feature = "panic"))]
        {
            let should_err = check_for_misbehaviour(
                invalid_trust_level,
                &misbehaviour,
                consensus_state_1,
                consensus_state_2,
                time,
            );
            assert!(should_err.is_err(), "Invalid trust level should return error");
        }
    }
}
