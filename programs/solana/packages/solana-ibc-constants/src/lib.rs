//! Constants and program IDs for IBC on Solana
//!
//! This crate provides all the program IDs and constants used by IBC on Solana

include!(concat!(env!("OUT_DIR"), "/generated_constants.rs"));

/// Anchor default discriminator length in bytes
pub const ANCHOR_DISCRIMINATOR_LEN: usize = 8;

/// ICS26 Router Program ID on Solana
pub const ICS26_ROUTER_ID: &str = "FRGF7cthWUvDvAHMUARUHFycyUK2VDUtBchmkwrz7hgx";

/// ICS07 Tendermint Light Client Program ID on Solana
pub const ICS07_TENDERMINT_ID: &str = "HqPcGpVHxNNFfVatjhG78dFVMwjyZixoKPdZSt3d3TdD";

/// Maximum size of chunk data for multi-transaction uploads.
/// Used by ics07-tendermint (header chunks) and ics26-router (payload/proof chunks).
pub const CHUNK_DATA_SIZE: usize = 900;

/// Number of static accounts for `AssembleAndUpdateClient` instruction
/// (excludes `remaining_accounts` for chunks/sigs).
/// Must match `AssembleAndUpdateClient::STATIC_ACCOUNTS` in ics07-tendermint program.
pub const ASSEMBLE_UPDATE_CLIENT_STATIC_ACCOUNTS: usize = 8;

/// IBC commitment version byte.
/// Used as prefix in packet/ack commitment calculations.
pub const IBC_VERSION: u8 = 0x02;
