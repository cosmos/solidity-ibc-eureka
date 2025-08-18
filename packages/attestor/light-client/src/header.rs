//! Attestor header types for IBC light client

use serde::{Deserialize, Serialize};

/// Minimal attestor header for client updates
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Header {
    /// The height this header is updating from (trusted height)
    pub new_height: u64,
    /// Timestamp of the new height
    pub timestamp: u64,
    /// Opaque abi-encoded data that was signed (abi.encode(packets))
    pub attestation_data: Vec<u8>,
    /// Signatures of the attestors
    pub signatures: Vec<Vec<u8>>, // 64-byte r||s (65 with optional v accepted)
    /// Compressed secp256k1 public keys (33 bytes) corresponding 1:1 with signatures
    pub public_keys: Vec<Vec<u8>>, // preferred over addresses
}

impl Header {}
