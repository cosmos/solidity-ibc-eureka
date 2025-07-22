//! Generic function and data structures for verifying
//! attested data.

use std::collections::HashSet;

use secp256k1::{ecdsa::Signature, hashes::Hash, Message, PublicKey};
use serde::Deserialize;

use crate::{client_state::ClientState, error::IbcAttestorClientError};

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

impl Verifyable {
    /// Returns a new instance of [Verifyable] with a valid
    /// set of [Vec<Signature>] and [Vec<PublicKey>].
    ///
    /// Fails if:
    /// - Signatures or pubkeys are not unique
    /// - Too few signatures or pubkeys are submitted
    /// - The number of signatures and pubkeys does not match
    pub fn new(
        attestation_data: Vec<u8>,
        signatures: Vec<Signature>,
        pubkeys: Vec<PublicKey>,
        client_state: &ClientState,
    ) -> Result<Self, IbcAttestorClientError> {
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

        Ok(Self {
            attestation_data,
            signatures,
            pubkeys,
        })
    }
}

/// Verifies the cryptographic validity of the [Verifyable] data
/// struct.
///
/// Fails if:
/// - The attestations cannot be verified
/// - A rogue public key is submitted
#[allow(clippy::module_name_repetitions)]
pub(crate) fn verify_attesation(
    client_state: &ClientState,
    verifiable: &Verifyable,
) -> Result<(), IbcAttestorClientError> {
    for (att_key, att_sig) in verifiable.pubkeys.iter().zip(&verifiable.signatures) {
        if let Some(lc_key) = client_state.pub_keys.iter().find(|k| k == &att_key) {
            let digest = secp256k1::hashes::sha256::Hash::hash(&verifiable.attestation_data);
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
