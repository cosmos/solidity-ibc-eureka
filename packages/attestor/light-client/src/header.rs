//! Attestor header types for IBC light client

use serde::{Deserialize, Serialize};

/// Minimal attestor header for client updates
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Header {
    /// The height this header is updating from (trusted height)
    pub new_height: u64,
    /// Timestamp of the new height
    pub timestamp: u64,
    /// ABI-encoded attestation data that was signed (bytes32[] for packet commitments or `StateAttestation` struct)
    pub attestation_data: Vec<u8>,
    /// Raw 65-byte signatures in (r||s||v) format for ECDSA address recovery
    pub signatures: Vec<Vec<u8>>,
}

impl Header {
    /// Create a new [Header] with ABI-encoded attestation data and raw signatures
    /// Signatures should be 65-byte (r||s||v) format for ECDSA address recovery
    #[must_use]
    pub const fn new(
        new_height: u64,
        timestamp: u64,
        attestation_data: Vec<u8>,
        signatures: Vec<Vec<u8>>,
    ) -> Self {
        Self {
            new_height,
            timestamp,
            attestation_data,
            signatures,
        }
    }
}
