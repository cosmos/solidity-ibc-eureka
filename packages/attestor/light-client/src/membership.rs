//! Membership proof verification for attestor client

use crate::{
    client_state::ClientState,
    consensus_state::ConsensusState,
    error::SolanaIBCError,
    verify_attestation::{self, Verifyable},
};

/// Verify membership proof - only works for heights that exist in consensus state
/// # Errors
/// Returns an error if the height is not found in consensus state or proof verification fails
pub fn verify_membership(
    consensus_state: &ConsensusState,
    client_state: &ClientState,
    height: u64,
    proof: Vec<u8>,
) -> Result<(), SolanaIBCError> {
    let attested_state: Verifyable =
        serde_json::from_slice(&proof).map_err(SolanaIBCError::DeserializeMembershipProofFailed)?;

    if consensus_state.height != height {
        return Err(SolanaIBCError::InvalidProof {
            reason: "heights must match".into(),
        });
    }

    let _ = verify_attestation::verify_attesation(client_state, &attested_state)?;

    Ok(())
}

/// Verify non-membership proof - only works for heights that exist in consensus state
/// # Errors
/// Returns an error if the height is not found in consensus state or proof verification fails
pub fn verify_non_membership(
    _consensus_state: &ConsensusState,
    _client_state: &ClientState,
    _height: u64,
    _proof: Vec<u8>,
) -> Result<(), SolanaIBCError> {
    todo!()
}
