#![doc = "Attestor light client for IBC"]
#![deny(
    clippy::nursery,
    clippy::pedantic,
    warnings,
    missing_docs,
    unused_crate_dependencies
)]

mod error;
mod packet_commitments;

pub mod verify_packet_membership;
pub use error::PacketAttestationError;
pub use packet_commitments::{PacketCommitments, PacketCompact};
