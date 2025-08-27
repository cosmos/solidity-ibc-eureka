#![doc = include_str!("../README.md")]
#![deny(
    clippy::nursery,
    clippy::pedantic,
    warnings,
    missing_docs,
    unused_crate_dependencies
)]
#![cfg_attr(test, allow(clippy::borrow_interior_mutable_const))]

/// ABI helpers for encoding/decoding attestation payloads
pub mod contract;
pub mod error;
pub mod instantiate;
pub mod msg;
pub mod query;
pub mod state;
pub mod sudo;

#[cfg(test)]
pub mod test;

pub use error::ContractError;

// ed25519-zebra is required by cosmwasm-crypto for signature verification
// with ed25519_zebra 4.1, batch was suddenly feature gated...
use ed25519_zebra as _;
