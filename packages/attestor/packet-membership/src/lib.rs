#![doc = "Attestor light client for IBC"]
#![deny(
    clippy::nursery,
    clippy::pedantic,
    warnings,
    missing_docs,
    unused_crate_dependencies
)]

mod error;
mod packets;

pub mod verify_packet_membership;
pub use error::PacketAttestationError;
pub use packets::Packets;
