//! Attestor header types for IBC light client

use k256::ecdsa::{Signature, VerifyingKey};
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
    pub pubkeys: Vec<VerifyingKey>,
}

impl Header {
    /// Create a new [Header] using encoded signatures and
    /// public keys.
    #[must_use]
    #[allow(clippy::missing_panics_doc)]
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
                .map(|bytes| Signature::try_from(bytes.as_slice()).unwrap())
                .collect(),
            pubkeys: pubkeys
                .into_iter()
                .map(|bytes| VerifyingKey::from_sec1_bytes(&bytes).unwrap())
                .collect(),
        }
    }
}
