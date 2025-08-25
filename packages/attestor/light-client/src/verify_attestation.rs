//! Generic function and data structures for verifying
//! attested data.

use std::collections::HashSet;

use k256::ecdsa::{signature::Verifier, Signature, VerifyingKey};
use sha2::{Digest, Sha256};

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
    attestation_data: &[u8],
    signatures: &[Signature],
    pubkeys: &[VerifyingKey],
) -> Result<(), IbcAttestorClientError> {
    let unique_sigs: HashSet<Vec<u8>> = signatures.iter().map(|s| s.to_bytes().to_vec()).collect();
    let unique_pubkeys: HashSet<Vec<u8>> =
        pubkeys.iter().map(|s| s.to_sec1_bytes().to_vec()).collect();

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

    for (att_key, att_sig) in pubkeys.iter().zip(signatures) {
        if let Some(lc_key) = client_state.pub_keys.iter().find(|k| k == &att_key) {
            let mut hasher = Sha256::new();
            hasher.update(attestation_data);
            let hash_result = hasher.finalize();

            lc_key
                .verify(&hash_result, att_sig)
                .map_err(|_| IbcAttestorClientError::InvalidSignature)?;
        } else {
            return Err(IbcAttestorClientError::UnknownPublicKeySubmitted { pubkey: *att_key });
        }
    }

    Ok(())
}
