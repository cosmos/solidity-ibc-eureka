//! The crate that contains the types and utilities for combined `tendermint-light-client` update client and membership verification.
#![deny(missing_docs, clippy::nursery, clippy::pedantic, warnings, unused_crate_dependencies)]

use ibc_client_tendermint::types::Header;
use ibc_core_commitment_types::merkle::MerkleProof;
use tendermint_light_client_membership::{KVPair, MembershipOutput};
use tendermint_light_client_update_client::{ClientState, ConsensusState, UpdateClientOutput};

/// Output from combined update client and membership verification
#[derive(Clone, Debug)]
pub struct UcAndMembershipOutput {
    /// Output from update client verification
    pub update_output: UpdateClientOutput,
    /// Output from membership verification
    pub membership_output: MembershipOutput,
}

/// Verify header update and membership proofs together
///
/// # Panics
/// Panics if either verification fails
#[allow(clippy::missing_panics_doc)]
#[must_use]
pub fn verify_uc_and_membership(
    client_state: ClientState,
    trusted_consensus_state: ConsensusState,
    proposed_header: Header,
    current_timestamp_nanos: u128,
    membership_requests: Vec<(KVPair, MerkleProof)>,
) -> UcAndMembershipOutput {
    // First perform update client verification
    let update_output = tendermint_light_client_update_client::verify_header_update(
        client_state,
        trusted_consensus_state,
        proposed_header.clone(),
        current_timestamp_nanos,
    );
    
    // Extract app hash from the proposed header for membership verification
    let app_hash: [u8; 32] = proposed_header
        .signed_header
        .header()
        .app_hash
        .as_bytes()
        .try_into()
        .unwrap();
    
    // Then perform membership verification
    let membership_output = tendermint_light_client_membership::verify_membership(
        app_hash,
        membership_requests,
    );
    
    UcAndMembershipOutput {
        update_output,
        membership_output,
    }
}