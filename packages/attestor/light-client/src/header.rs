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
    /// Opaque borsh-encoded data that was signed
    pub attestation_data: Vec<u8>,
    /// Signatures of the attestors
    pub signatures: Vec<Signature>,
    /// Public keys of the attestors submitting attestations
    pub pubkeys: Vec<PublicKey>,
}
