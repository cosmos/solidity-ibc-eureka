//! Membership proof verification for attestor client

use secp256k1::{ecdsa::Signature, PublicKey};
use serde::Deserialize;

use crate::{
    client_state::ClientState, consensus_state::ConsensusState, error::IbcAttestorClientError,
    verify_attestation,
};

/// Data structure that can be verified cryptographically
#[derive(Deserialize)]
pub struct Verifyable {
    /// Opaque borsh-encoded data that was signed
    attestation_data: Vec<u8>,
    /// Signatures of the attestors
    signatures: Vec<Signature>,
    /// Public keys of the attestors submitting attestations
    pubkeys: Vec<PublicKey>,
}

/// Verify membership proof - only works for heights that exist in consensus state
/// # Errors
/// Returns an error if the height is not found in consensus state or proof verification fails
pub fn verify_membership(
    consensus_state: &ConsensusState,
    client_state: &ClientState,
    height: u64,
    proof: Vec<u8>,
) -> Result<(), IbcAttestorClientError> {
    let attested_state: Verifyable = serde_json::from_slice(&proof)
        .map_err(IbcAttestorClientError::DeserializeMembershipProofFailed)?;

    if consensus_state.height != height {
        return Err(IbcAttestorClientError::InvalidProof {
            reason: "heights must match".into(),
        });
    }

    let _ = verify_attestation::verify_attestation(
        client_state,
        attested_state.attestation_data,
        attested_state.signatures,
        attested_state.pubkeys,
    )?;

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
) -> Result<(), IbcAttestorClientError> {
    todo!()
}
