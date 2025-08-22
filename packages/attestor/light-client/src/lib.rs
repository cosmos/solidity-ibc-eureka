#![doc = "Attestor light client for IBC"]
#![deny(
	clippy::nursery,
	clippy::pedantic,
	warnings,
	missing_docs,
	unused_crate_dependencies
)]

use sha3 as _;

pub mod client_state;
pub mod consensus_state;
pub mod error;
pub mod header;
pub mod membership;
pub mod update;
pub mod verify;
pub mod verify_attestation;

#[cfg(any(test, feature = "test-utils"))]
pub mod test_utils;
