//! Generic function and data structures for verifying
//! attested data.

use std::collections::HashSet;

use secp256k1::{ecdsa::Signature, hashes::Hash, Message, PublicKey};

use crate::{client_state::ClientState, error::IbcAttestorClientError};

/// Verifies the cryptographic validity of the attestation data.
///
/// Fails if:
/// - Signatures or pubkeys are not unique
/// - Too few signatures or pubkeys are submitted
/// - The number of signatures and pubkeys does not match
/// - The attestations cannot be verified
/// - A rogue public key is submitted
#[allow(clippy::module_name_repetitions)]
pub(crate) fn verify_attestation(
    client_state: &ClientState,
    attestation_data: Vec<u8>,
    signatures: Vec<Signature>,
    pubkeys: Vec<PublicKey>,
) -> Result<(), IbcAttestorClientError> {
    let unique_sigs: HashSet<Signature> = signatures.iter().cloned().collect();
    let unique_pubkeys: HashSet<PublicKey> = pubkeys.iter().cloned().collect();

    if unique_sigs.len() != signatures.len()
        || unique_sigs.len() < client_state.min_required_sigs as usize
    {
        return Err(IbcAttestorClientError::InvalidAttestedData {
            reason: "too few or duplicate signatures provided".into(),
        });
    }
    if unique_pubkeys.len() != pubkeys.len()
        || unique_pubkeys.len() < client_state.min_required_sigs as usize
    {
        return Err(IbcAttestorClientError::InvalidAttestedData {
            reason: "too few or duplicate public keys provided".into(),
        });
    }
    if signatures.len() != pubkeys.len() {
        return Err(IbcAttestorClientError::InvalidAttestedData {
            reason: "number of signatures do not match attestations".into(),
        });
    }

    for (att_key, att_sig) in pubkeys.iter().zip(&signatures) {
        if let Some(lc_key) = client_state.pub_keys.iter().find(|k| k == &att_key) {
            let digest = secp256k1::hashes::sha256::Hash::hash(&attestation_data);
            let message = Message::from_digest(digest.to_byte_array());
            let _ = att_sig
                .verify(message, lc_key)
                .map_err(|_| IbcAttestorClientError::InvalidSignature)?;
        } else {
            return Err(IbcAttestorClientError::UnknownPublicKeySubmitted { pubkey: *att_key });
        }
    }

    Ok(())
}
