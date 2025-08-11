//! Attestor header types for IBC light client

use secp256k1::{ecdsa::Signature, PublicKey};
use serde::{Deserialize, Serialize};

/// Minimal attestor header for client updates
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Header {
    /// The height this header is updating from (trusted height)
    pub new_height: u64,
    /// Timestamp of the new height
    pub timestamp: u64,
    /// Opaque serde-encoded data that was signed
    pub attestation_data: Vec<u8>,
    /// Signatures of the attestors
    pub signatures: Vec<Signature>,
    /// Public keys of the attestors submitting attestations
    pub pubkeys: Vec<PublicKey>,
}

impl Header {
    /// Create a new [Header] using encoded signatures and
    /// public keys.
    pub fn new(
        new_height: u64,
        timestamp: u64,
        attestation_data: Vec<u8>,
        signatures: Vec<Vec<u8>>,
        pubkeys: Vec<Vec<u8>>,
    ) -> Self {
        Self {
            new_height,
            timestamp,
            attestation_data,
            // TODO: Make this fallable
            signatures: signatures
                .into_iter()
                .map(|bytes| Signature::from_compact(&bytes).unwrap())
                .collect(),
            pubkeys: pubkeys
                .into_iter()
                .map(|bytes| PublicKey::from_slice(&bytes).unwrap())
                .collect(),
        }
    }
}
