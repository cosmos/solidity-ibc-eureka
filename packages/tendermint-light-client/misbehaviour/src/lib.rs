//! The crate that contains the types and utilities for `tendermint-light-client-update-client`
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
use ibc_client_tendermint::types::{ConsensusState, Misbehaviour, TENDERMINT_CLIENT_TYPE};
use ibc_core_host_types::identifiers::{ChainId, ClientId};
use ibc_eureka_solidity_types::msgs::{
    IICS07TendermintMsgs::ClientState, IMisbehaviourMsgs::MisbehaviourOutput,
};
use std::time::Duration;
use tendermint_light_client_update_client::types::validation::ClientValidationCtx;
use tendermint_light_client_verifier::options::Options;
use tendermint_light_client_verifier::ProdVerifier;

/// The main function of the program without the zkVM wrapper.
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
    assert_eq!(
        client_state.chainId,
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

    let options = Options {
        trust_threshold: client_state.trustLevel.clone().into(),
        trusting_period: Duration::from_secs(client_state.trustingPeriod.into()),
        clock_drift: Duration::from_secs(15),
    };

    // Call into ibc-rs verify_misbehaviour function to verify that both headers are valid given their respective trusted consensus states
    verify_misbehaviour::<_, sha2::Sha256>(
        &ctx,
        misbehaviour,
        &client_id,
        &ChainId::new(&client_state.chainId).unwrap(),
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
        clientState: client_state,
        trustedHeight1: misbehaviour.header1().trusted_height.into(),
        trustedHeight2: misbehaviour.header2().trusted_height.into(),
        trustedConsensusState1: trusted_consensus_state_1.into(),
        trustedConsensusState2: trusted_consensus_state_2.into(),
        time,
    }
}
