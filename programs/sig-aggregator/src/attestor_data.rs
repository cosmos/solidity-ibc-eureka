use crate::rpc::{AggregateResponse, AttestationsFromHeightResponse, SigPubkeyPair};
use alloy_primitives::FixedBytes;
use std::collections::HashMap;

type Height = u64;

pub const STATE_BYTE_LENGTH: usize = 12;
type State = FixedBytes<STATE_BYTE_LENGTH>;

// https://docs.rs/secp256k1/latest/secp256k1/ecdsa/struct.Signature.html#method.serialize_compact
pub const SIGNATURE_BYTE_LENGTH: usize = 64;
type Signature = FixedBytes<SIGNATURE_BYTE_LENGTH>;

// Compressed public key length
// https://docs.rs/secp256k1/latest/secp256k1/struct.PublicKey.html#method.serialize
pub const PUBKEY_BYTE_LENGTH: usize = 33;
type Pubkey = FixedBytes<PUBKEY_BYTE_LENGTH>;

/// Maps height -> state -> list of (signature, pubkey) pairs
/// 
/// Structure:
/// ```text
/// Height: 101
///   State: 0x1234... (12 bytes)
///     [(signature_A, pubkey_A), (signature_B, pubkey_B)]
///   State: 0x9876...
///     [(signature_C, pubkey_C), (signature_D, pubkey_D)]
/// Height: 102
///   State: 0x5678...
///     [(signature_A, pubkey_A), (signature_B, pubkey_B), ...]
/// ```
type SignedStates = HashMap<State, Vec<(Signature, Pubkey)>>;

#[derive(Debug, Clone, Default)]
pub struct AttestatorData {
    height_states: HashMap<Height, SignedStates>,
}

impl AttestatorData {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, response: AttestationsFromHeightResponse) {
        let pubkey = match Pubkey::try_from(response.pubkey.as_slice()) {
            Ok(pk) => pk,
            Err(_) => {
                tracing::warn!("Invalid pubkey length: {}", response.pubkey.len());
                return;
            }
        };

        for attestation in response.attestations {
            let state = match State::try_from(attestation.data.as_slice()) {
                Ok(s) => s,
                Err(_) => {
                    tracing::warn!("Invalid state length: {}", attestation.data.len());
                    continue;
                }
            };

            let signature = match Signature::try_from(attestation.signature.as_slice()) {
                Ok(sig) => sig,
                Err(_) => {
                    tracing::warn!("Invalid signature length: {}", attestation.signature.len());
                    continue;
                }
            };

            self.height_states
                .entry(attestation.height)
                .or_default()
                .entry(state)
                .or_default()
                .push((signature, pubkey));
        }
    }

    #[must_use]
    pub fn get_latest(&self, quorum: usize) -> Option<AggregateResponse> {
        let mut best_response: Option<AggregateResponse> = None;

        for (&height, state_map) in &self.height_states {
            if let Some(ref current_best) = best_response {
                if height <= current_best.height {
                    continue;
                }
            }

            for (&state, sig_pubkey_pairs) in state_map {
                if sig_pubkey_pairs.len() < quorum {
                    continue;
                }

                let response = AggregateResponse {
                    height,
                    state: state.to_vec(),
                    sig_pubkey_pairs: sig_pubkey_pairs
                        .iter()
                        .map(|(signature, pubkey)| SigPubkeyPair {
                            sig: signature.to_vec(),
                            pubkey: pubkey.to_vec(),
                        })
                        .collect(),
                };

                best_response = Some(response);
            }
        }

        best_response
    }
}
