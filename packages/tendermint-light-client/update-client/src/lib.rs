//! The crate that contains the types and utilities for `tendermint-light-client-update-client`
//! program.
#![deny(missing_docs, clippy::nursery, clippy::pedantic, warnings, unused_crate_dependencies)]

pub mod types;

use std::{str::FromStr, time::Duration};

use ibc_client_tendermint::{
    client_state::verify_header,
    types::{ConsensusState, Header, TENDERMINT_CLIENT_TYPE},
};
use ibc_core_client_types::Height;
use ibc_core_host_types::identifiers::{ChainId, ClientId};
use tendermint_light_client_verifier::{options::Options, ProdVerifier, types::TrustThreshold};

#[cfg(feature = "ethereum")]
mod ethereum;

#[cfg(feature = "ethereum")]
pub use ethereum::*;

#[cfg(feature = "solana")]
mod solana;

#[cfg(feature = "solana")]
pub use solana::*;

/// Trait for abstracting client state information across different platforms
pub trait ClientStateInfo {
    /// Get the chain ID
    fn chain_id(&self) -> &str;
    /// Get the trust threshold
    fn trust_level(&self) -> TrustThreshold;
    /// Get the trusting period in seconds
    fn trusting_period(&self) -> u64;
}

/// Trait for constructing platform-specific outputs
pub trait UpdateClientOutputInfo<CS> {
    /// Create output from verification results
    fn from_verification(
        client_state: CS,
        trusted_consensus_state: ConsensusState,
        new_consensus_state: ConsensusState,
        time: u128,
        trusted_height: Height,
        new_height: Height,
    ) -> Self;
}

/// Core update client logic
#[allow(clippy::missing_panics_doc)]
pub fn update_client_core<CS, O>(
    client_state: CS,
    trusted_consensus_state: ConsensusState,
    proposed_header: Header,
    time: u128,
) -> O
where
    CS: ClientStateInfo,
    O: UpdateClientOutputInfo<CS>,
{
    let client_id = ClientId::new(TENDERMINT_CLIENT_TYPE, 0).unwrap();
    let chain_id = ChainId::from_str(client_state.chain_id()).unwrap();
    let options = Options {
        trust_threshold: client_state.trust_level(),
        trusting_period: Duration::from_secs(client_state.trusting_period()),
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

    let trusted_height = proposed_header.trusted_height;
    let new_height = proposed_header.height();
    let new_consensus_state = ConsensusState::from(proposed_header);

    O::from_verification(
        client_state,
        trusted_consensus_state,
        new_consensus_state,
        time,
        trusted_height,
        new_height,
    )
}
