//! Re-exports from specialized modules for backward compatibility.
//!
//! This module serves as a facade, re-exporting functions from:
//! - `abi_decode`: Ethereum ABI decoding for attestation types
//! - `verification`: Signature verification logic
//! - `crypto`: Cryptographic primitives (secp256k1, keccak256)
//! - `proof`: Membership proof deserialization

pub use crate::abi_decode::{decode_packet_attestation, decode_state_attestation};
pub use crate::crypto::{hash_path, keccak256, recover_eth_address};
pub use crate::proof::deserialize_membership_proof;
pub use crate::verification::verify_attestation;
