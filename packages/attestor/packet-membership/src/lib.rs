#![doc = "Attestor light client for IBC"]
#![deny(
    clippy::nursery,
    clippy::pedantic,
    warnings,
    missing_docs,
    unused_crate_dependencies
)]

mod error;

pub mod verify_packet_membership;
pub use error::PacketAttestationError;
