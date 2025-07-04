//! The crate that contains the types and utilities for `tendermint-light-client-update-client`
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

pub mod types;

use std::{str::FromStr, time::Duration};

use ibc_client_tendermint::{
    client_state::verify_header,
    types::{ConsensusState, Header, TENDERMINT_CLIENT_TYPE},
};
use ibc_core_client_types::Height;
use ibc_core_host_types::identifiers::{ChainId, ClientId};
use tendermint_light_client_verifier::{options::Options, types::TrustThreshold as TmTrustThreshold, ProdVerifier};

/// Trust threshold
#[derive(Clone, Debug)]
pub struct TrustThreshold {
    /// Numerator of the fraction
    pub numerator: u64,
    /// Denominator of the fraction
    pub denominator: u64,
}

impl TrustThreshold {
    /// Create a new trust threshold
    #[must_use]
    pub const fn new(numerator: u64, denominator: u64) -> Self {
        Self { numerator, denominator }
    }
}

impl From<TrustThreshold> for TmTrustThreshold {
    fn from(tt: TrustThreshold) -> Self {
        TmTrustThreshold::new(tt.numerator, tt.denominator)
            .expect("trust threshold numerator must be less than or equal to denominator")
    }
}

/// Client state
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


/// Output from update client verification
#[derive(Clone, Debug)]
pub struct UpdateClientOutput {
    /// New client state (with updated latest height)
    pub new_client_state: ClientState,
    /// New consensus state from the verified header
    pub new_consensus_state: ConsensusState,
    /// The trusted height used for verification
    pub trusted_height: Height,
}

/// Error type for update client (only used when panic feature is not enabled)
#[cfg(not(feature = "panic"))]
#[derive(Clone, Debug, thiserror::Error)]
pub enum UpdateClientError {
    /// Invalid trust threshold
    #[error("invalid trust threshold: numerator must be less than or equal to denominator")]
    InvalidTrustThreshold,
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
    /// Header verification failed
    #[error("header verification failed: {0}")]
    HeaderVerificationFailed(#[source] ibc_client_tendermint::client_state::Error),
}

/// IBC light client update client - panic version
#[cfg(feature = "panic")]
#[allow(clippy::missing_panics_doc)]
#[must_use]
pub fn update_client(
    client_state: ClientState,
    trusted_consensus_state: ConsensusState,
    proposed_header: Header,
    time: u128,
) -> UpdateClientOutput {
    let client_id = ClientId::new(TENDERMINT_CLIENT_TYPE, 0).unwrap();
    let chain_id = ChainId::from_str(&client_state.chain_id).unwrap();

    let trust_threshold: TmTrustThreshold = client_state.trust_level.clone().into();

    let options = Options {
        trust_threshold,
        trusting_period: Duration::from_secs(client_state.trusting_period_seconds),
        clock_drift: Duration::from_secs(client_state.max_clock_drift_seconds),
    };

    let mut ctx = types::validation::ClientValidationCtx::new(time);

    ctx.insert_trusted_consensus_state(
        client_id.clone(),
        proposed_header.trusted_height.revision_number(),
        proposed_header.trusted_height.revision_height(),
        &trusted_consensus_state,
    );

    verify_header::<_, sha2::Sha256>(
        &ctx,
        &proposed_header,
        &client_id,
        &chain_id,
        &options,
        &ProdVerifier::default(),
    )
    .unwrap();

    let trusted_height = proposed_header.trusted_height;
    let new_height = proposed_header.height();
    let new_consensus_state = ConsensusState::from(proposed_header);

    UpdateClientOutput {
        new_client_state: ClientState {
            latest_height: new_height,
            ..client_state
        },
        new_consensus_state,
        trusted_height,
    }
}

/// IBC light client update client - non-panic version
#[cfg(not(feature = "panic"))]
#[must_use]
pub fn update_client(
    client_state: ClientState,
    trusted_consensus_state: ConsensusState,
    proposed_header: Header,
    time: u128,
) -> Result<UpdateClientOutput, UpdateClientError> {
    let client_id = ClientId::new(TENDERMINT_CLIENT_TYPE, 0)
        .map_err(UpdateClientError::InvalidClientId)?;
    let chain_id = ChainId::from_str(&client_state.chain_id)
        .map_err(|e| UpdateClientError::InvalidChainId {
            chain_id: client_state.chain_id.clone(),
            source: e,
        })?;

    let trust_threshold: TmTrustThreshold = TmTrustThreshold::new(
        client_state.trust_level.numerator,
        client_state.trust_level.denominator,
    )
    .ok_or(UpdateClientError::InvalidTrustThreshold)?;

    let options = Options {
        trust_threshold,
        trusting_period: Duration::from_secs(client_state.trusting_period_seconds),
        clock_drift: Duration::from_secs(client_state.max_clock_drift_seconds),
    };

    let mut ctx = types::validation::ClientValidationCtx::new(time);

    ctx.insert_trusted_consensus_state(
        client_id.clone(),
        proposed_header.trusted_height.revision_number(),
        proposed_header.trusted_height.revision_height(),
        &trusted_consensus_state,
    );

    verify_header::<_, sha2::Sha256>(
        &ctx,
        &proposed_header,
        &client_id,
        &chain_id,
        &options,
        &ProdVerifier::default(),
    )
    .map_err(UpdateClientError::HeaderVerificationFailed)?;

    let trusted_height = proposed_header.trusted_height;
    let new_height = proposed_header.height();
    let new_consensus_state = ConsensusState::from(proposed_header);

    Ok(UpdateClientOutput {
        new_client_state: ClientState {
            latest_height: new_height,
            ..client_state
        },
        new_consensus_state,
        trusted_height,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn test_trust_threshold_conversion() {
        let tt = TrustThreshold::new(1, 3);
        let tm_tt: TmTrustThreshold = tt.into();
        assert_eq!(tm_tt.numerator(), 1);
        assert_eq!(tm_tt.denominator(), 3);
    }

    #[test]
    #[should_panic(expected = "trust threshold numerator must be less than or equal to denominator")]
    fn test_invalid_trust_threshold_panics() {
        let tt = TrustThreshold::new(3, 1); // numerator > denominator
        let _tm_tt: TmTrustThreshold = tt.into();
    }

    #[test]
    fn test_panic_and_non_panic_modes_fail_with_invalid_empty_chain_id() {
        let client_state = test_client_state();

        #[cfg(feature = "panic")]
        {
            // With panic feature, invalid inputs will cause panic
            let invalid_client_state = ClientState {
                chain_id: "".to_string(), // Invalid empty chain ID
                ..client_state
            };

            let result = std::panic::catch_unwind(|| {
                update_client(
                    invalid_client_state,
                    ConsensusState::default(),
                    Header::default(),
                    0,
                )
            });
            assert!(result.is_err(), "Expected panic for invalid chain ID");
        }

        #[cfg(not(feature = "panic"))]
        {
            // Without panic feature, invalid inputs return Err
            let invalid_client_state = ClientState {
                chain_id: "".to_string(), // Invalid empty chain ID
                ..client_state
            };

            let result = update_client(
                invalid_client_state,
                ConsensusState::default(),
                Header::default(),
                0,
            );
            assert!(result.is_err());
            match result {
                Err(UpdateClientError::InvalidChainId { .. }) => {
                    // Expected error
                }
                _ => panic!("Unexpected error type"),
            }
        }
    }

    #[test]
    fn test_client_state_fields() {
        let client_state = test_client_state();
        assert_eq!(client_state.chain_id, "test-chain");
        assert_eq!(client_state.trust_level.numerator, 1);
        assert_eq!(client_state.trust_level.denominator, 3);
        assert_eq!(client_state.trusting_period_seconds, 3600);
        assert_eq!(client_state.unbonding_period_seconds, 7200);
        assert_eq!(client_state.max_clock_drift_seconds, 60);
        assert!(client_state.frozen_height.is_none());
        assert_eq!(client_state.latest_height.revision_number(), 1);
        assert_eq!(client_state.latest_height.revision_height(), 100);
    }
}
