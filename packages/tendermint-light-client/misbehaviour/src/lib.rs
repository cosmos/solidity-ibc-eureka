//! The crate that contains the types and utilities for `tendermint-light-client-update-client`
//! program.
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

/// IBC light client misbehaviour check.
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
