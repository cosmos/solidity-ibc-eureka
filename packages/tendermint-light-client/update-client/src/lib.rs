//! The crate that contains the types and utilities for `tendermint-light-client-update-client`
//! program.
#![deny(missing_docs, clippy::nursery, clippy::pedantic, warnings, unused_crate_dependencies)]

pub mod types;

use std::{str::FromStr, time::Duration};

use ibc_client_tendermint::client_state::verify_header;
use ibc_client_tendermint::types::{ConsensusState as TmConsensusState, Header, TENDERMINT_CLIENT_TYPE};
use ibc_core_client_types::Height;
use ibc_core_host_types::identifiers::{ChainId, ClientId};
use tendermint_light_client_verifier::{options::Options, types::TrustThreshold, ProdVerifier};

/// Platform-agnostic client state
#[derive(Clone, Debug)]
pub struct ClientState {
    /// Chain ID
    pub chain_id: String,
    /// Trust level numerator
    pub trust_level_numerator: u64,
    /// Trust level denominator
    pub trust_level_denominator: u64,
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

/// Platform-agnostic consensus state
#[derive(Clone, Debug)]
pub struct ConsensusState {
    /// Timestamp in nanoseconds
    pub timestamp_nanos: u128,
    /// App hash
    pub app_hash: [u8; 32],
    /// Next validators hash
    pub next_validators_hash: [u8; 32],
}

impl From<ConsensusState> for TmConsensusState {
    fn from(cs: ConsensusState) -> Self {
        TmConsensusState {
            root: cs.app_hash.to_vec().try_into().expect("valid app hash"),
            next_validators_hash: cs.next_validators_hash.into(),
            timestamp: ibc_core_client_types::timestamp::Timestamp::from_nanoseconds(cs.timestamp_nanos)
                .expect("valid timestamp"),
        }
    }
}

impl From<TmConsensusState> for ConsensusState {
    fn from(cs: TmConsensusState) -> Self {
        Self {
            timestamp_nanos: cs.timestamp.nanoseconds(),
            app_hash: cs.root.into_vec().try_into().expect("32 byte app hash"),
            next_validators_hash: cs.next_validators_hash.as_bytes().try_into().expect("32 byte hash"),
        }
    }
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

/// Verify a tendermint header update
///
/// # Panics
/// Panics if header verification fails
#[allow(clippy::missing_panics_doc)]
#[must_use]
pub fn verify_header_update(
    client_state: ClientState,
    trusted_consensus_state: ConsensusState,
    proposed_header: Header,
    current_timestamp_nanos: u128,
) -> UpdateClientOutput {
    let client_id = ClientId::new(TENDERMINT_CLIENT_TYPE, 0).unwrap();
    let chain_id = ChainId::from_str(&client_state.chain_id).unwrap();
    
    let trust_threshold = TrustThreshold::new(
        client_state.trust_level_numerator,
        client_state.trust_level_denominator,
    )
    .unwrap();
    
    let options = Options {
        trust_threshold,
        trusting_period: Duration::from_secs(client_state.trusting_period_seconds),
        clock_drift: Duration::from_secs(client_state.max_clock_drift_seconds),
    };

    let mut ctx = types::validation::ClientValidationCtx::new(current_timestamp_nanos);
    
    // Convert our ConsensusState to tendermint ConsensusState
    let tm_consensus_state: TmConsensusState = trusted_consensus_state.into();
    
    ctx.insert_trusted_consensus_state(
        client_id.clone(),
        proposed_header.trusted_height.revision_number(),
        proposed_header.trusted_height.revision_height(),
        &tm_consensus_state,
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
    let new_consensus_state: ConsensusState = TmConsensusState::from(proposed_header).into();

    UpdateClientOutput {
        new_client_state: ClientState {
            latest_height: new_height,
            ..client_state
        },
        new_consensus_state,
        trusted_height,
    }
}