//! Membership proof verification for attestor client

use secp256k1::{ecdsa::Signature, hashes::Hash, Message, PublicKey};
use serde::Deserialize;

use crate::{client_state::ClientState, consensus_state::ConsensusState, error::SolanaIBCError};

#[derive(Deserialize)]
struct Verifyable {
    /// Opaque borsh-encoded data that was signed
    pub attestation_data: Vec<u8>,
    /// Signatures of the attestors
    pub signatures: Vec<Signature>,
    /// Public keys of the attestors submitting attestations
    pub pubkeys: Vec<PublicKey>,
}

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

    for (att_key, att_sig) in attested_state.pubkeys.iter().zip(attested_state.signatures) {
        if let Some(lc_key) = client_state.pub_keys.iter().find(|k| k == &att_key) {
            let digest = secp256k1::hashes::sha256::Hash::hash(&attested_state.attestation_data);
            let message = Message::from_digest(digest.to_byte_array());
            let _ = att_sig
                .verify(message, lc_key)
                .map_err(|_| SolanaIBCError::InvalidSignature)?;
        } else {
            return Err(SolanaIBCError::InvalidProof {
                reason: "unknown pubkey in proof".into(),
            });
        }
    }

    Ok(())
}

/// Verify non-membership proof - only works for heights that exist in consensus state
/// # Errors
/// Returns an error if the height is not found in consensus state or proof verification fails
pub fn verify_non_membership(
    _trusted_consensus: &ConsensusState,
    _client_state: &ClientState,
    _proof: Vec<u8>,
) -> Result<(), SolanaIBCError> {
    todo!()
}
