//! The crate that contains the types and utilities for `tendermint-light-client-membership` program.
#![deny(missing_docs, clippy::nursery, clippy::pedantic, warnings, unused_crate_dependencies)]

use ibc_client_tendermint_types::{ConsensusState, Header};
use ibc_core_commitment_types::merkle::MerkleProof;
use tendermint_light_client_update_client::{ClientStateInfo, UpdateClientOutputInfo};
use tendermint_light_client_membership::{KVPairInfo, MembershipOutputInfo};

#[cfg(feature = "ethereum")]
mod ethereum;

#[cfg(feature = "ethereum")]
pub use ethereum::*;

#[cfg(feature = "solana")]
mod solana;

#[cfg(feature = "solana")]
pub use solana::*;

/// Trait for constructing platform-specific UC and membership outputs
pub trait UcAndMembershipOutputInfo<CS, K> {
    /// The update client output type
    type UpdateClientOutput: UpdateClientOutputInfo<CS>;
    /// The membership output type
    type MembershipOutput: MembershipOutputInfo<K>;

    /// Create output from both verification results
    fn from_results(
        uc_output: Self::UpdateClientOutput,
        membership_output: Self::MembershipOutput,
    ) -> Self;
}

/// Core update client and membership logic
#[allow(clippy::missing_panics_doc, dead_code)]
fn update_client_and_membership_core<CS, K, O>(
    client_state: CS,
    trusted_consensus_state: ConsensusState,
    proposed_header: Header,
    time: u128,
    request_iter: impl Iterator<Item = (K, MerkleProof)>,
) -> O
where
    CS: ClientStateInfo,
    K: KVPairInfo,
    O: UcAndMembershipOutputInfo<CS, K>,
{
    let app_hash: [u8; 32] = proposed_header
        .signed_header
        .header()
        .app_hash
        .as_bytes()
        .try_into()
        .unwrap();

    let uc_output = tendermint_light_client_update_client::update_client_generic::<CS, O::UpdateClientOutput>(
        client_state,
        trusted_consensus_state,
        proposed_header,
        time,
    );

    let mem_output = tendermint_light_client_membership::membership_generic::<K, O::MembershipOutput>(
        app_hash,
        request_iter,
    );

    O::from_results(uc_output, mem_output)
}
