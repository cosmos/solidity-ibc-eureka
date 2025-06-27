//! The crate that contains the types and utilities for `tendermint-light-client-update-client`
//! program.
#![deny(missing_docs, clippy::nursery, clippy::pedantic, warnings)]

pub mod types;

use std::{str::FromStr, time::Duration};

use ibc_client_tendermint::{
    client_state::verify_header,
    types::{ConsensusState, Header, TENDERMINT_CLIENT_TYPE},
};
use ibc_core_host_types::identifiers::{ChainId, ClientId};
use ibc_eureka_solidity_types::msgs::{
    IICS07TendermintMsgs::ClientState, IUpdateClientMsgs::UpdateClientOutput,
};

use tendermint_light_client_verifier::{options::Options, ProdVerifier};

/// The main function of the program without the zkVM wrapper.
#[allow(clippy::missing_panics_doc)]
#[must_use]
pub fn update_client(
    client_state: ClientState,
    trusted_consensus_state: ConsensusState,
    proposed_header: Header,
    time: u128,
) -> UpdateClientOutput {
    let client_id = ClientId::new(TENDERMINT_CLIENT_TYPE, 0).unwrap();
    let chain_id = ChainId::from_str(&client_state.chainId).unwrap();
    let options = Options {
        trust_threshold: client_state.trustLevel.clone().into(),
        trusting_period: Duration::from_secs(client_state.trustingPeriod.into()),
        clock_drift: Duration::from_secs(15),
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

    let trusted_height = proposed_header.trusted_height.into();
    let new_height = proposed_header.height().into();
    let new_consensus_state = ConsensusState::from(proposed_header);

    UpdateClientOutput {
        clientState: client_state,
        trustedConsensusState: trusted_consensus_state.into(),
        newConsensusState: new_consensus_state.into(),
        time,
        trustedHeight: trusted_height,
        newHeight: new_height,
    }
}
