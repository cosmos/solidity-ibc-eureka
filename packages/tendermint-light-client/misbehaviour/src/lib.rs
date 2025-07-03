//! The crate that contains the types and utilities for `tendermint-light-client-misbehaviour`
//! program.
#![deny(
    missing_docs,
    clippy::nursery,
    clippy::pedantic,
    warnings,
    unused_crate_dependencies
)]

use ibc_client_tendermint::client_state::{
    check_for_misbehaviour_on_misbehavior, verify_misbehaviour,
};
use ibc_client_tendermint::types::{ConsensusState, Header, Misbehaviour, TENDERMINT_CLIENT_TYPE};
use ibc_core_client_types::Height;
use ibc_core_host_types::identifiers::{ChainId, ClientId};
use std::{str::FromStr, time::Duration};
use tendermint_light_client_verifier::{options::Options, types::TrustThreshold, ProdVerifier};

// Import validation context from update-client crate
use tendermint_light_client_update_client::types::validation::ClientValidationCtx;

/// Platform-agnostic client state for misbehaviour detection
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

/// Output from misbehaviour verification
#[derive(Clone, Debug)]
pub struct MisbehaviourOutput {
    /// The height at which the client should be frozen
    pub frozen_height: Height,
}

/// Verify misbehaviour in tendermint headers
///
/// # Panics
/// Panics if misbehaviour verification fails or no misbehaviour is detected
#[allow(clippy::missing_panics_doc)]
#[must_use]
pub fn verify_misbehaviour(
    client_state: ClientState,
    header_1: Header,
    header_2: Header,
) -> MisbehaviourOutput {
    let client_id = ClientId::new(TENDERMINT_CLIENT_TYPE, 0).unwrap();
    let chain_id = ChainId::from_str(&client_state.chain_id).unwrap();
    
    // Create Misbehaviour from headers
    let misbehaviour = Misbehaviour::new(header_1.clone(), header_2.clone());
    
    assert_eq!(
        client_state.chain_id,
        misbehaviour
            .header1()
            .signed_header
            .header
            .chain_id
            .to_string()
    ); // header2 is checked by `verify_misbehaviour`

    // Create context and insert consensus states from headers
    let mut ctx = ClientValidationCtx::new(0); // Time doesn't matter for misbehaviour
    
    // Insert consensus state for header1's trusted height
    let cs1 = ConsensusState::from(header_1.clone());
    ctx.insert_trusted_consensus_state(
        client_id.clone(),
        misbehaviour.header1().trusted_height.revision_number(),
        misbehaviour.header1().trusted_height.revision_height(),
        &cs1,
    );
    
    // Insert consensus state for header2's trusted height
    let cs2 = ConsensusState::from(header_2.clone());
    ctx.insert_trusted_consensus_state(
        client_id.clone(),
        misbehaviour.header2().trusted_height.revision_number(),
        misbehaviour.header2().trusted_height.revision_height(),
        &cs2,
    );

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

    // Call into ibc-rs verify_misbehaviour function to verify that both headers are valid
    verify_misbehaviour::<_, sha2::Sha256>(
        &ctx,
        &misbehaviour,
        &client_id,
        &chain_id,
        &options,
        &ProdVerifier::default(),
    )
    .unwrap();

    // Check for actual misbehaviour
    let is_misbehaviour =
        check_for_misbehaviour_on_misbehavior(misbehaviour.header1(), misbehaviour.header2())
            .unwrap();
    assert!(is_misbehaviour, "Misbehaviour is not detected");

    // Return the height at which to freeze the client
    // Use the minimum height of the two headers
    let frozen_height = std::cmp::min(
        misbehaviour.header1().height(),
        misbehaviour.header2().height(),
    );
    
    MisbehaviourOutput { frozen_height }
}