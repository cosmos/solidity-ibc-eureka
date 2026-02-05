//! Constants and program IDs for IBC on Solana
//!
//! This crate provides all the program IDs and constants used by IBC on Solana

include!(concat!(env!("OUT_DIR"), "/generated_constants.rs"));

/// Anchor default discriminator length in bytes
pub const ANCHOR_DISCRIMINATOR_LEN: usize = 8;

// TODO: Should we load these IDs from ENV variables?

/// ICS26 Router Program ID on Solana
pub const ICS26_ROUTER_ID: &str = "FRGF7cthWUvDvAHMUARUHFycyUK2VDUtBchmkwrz7hgx";

/// ICS07 Tendermint Light Client Program ID on Solana
pub const ICS07_TENDERMINT_ID: &str = "HqPcGpVHxNNFfVatjhG78dFVMwjyZixoKPdZSt3d3TdD";

/// Client type prefix for ICS07 Tendermint light client
pub const CLIENT_TYPE_TENDERMINT: &str = "07-tendermint";

/// Client type prefix for attestations light client (follows ibc-go convention)
pub const CLIENT_TYPE_ATTESTATION: &str = "attestations";

/// Extracts the client type from a client ID.
///
/// IBC client IDs follow the format `{client_type}-{sequence}`.
/// For example: `07-tendermint-0`, `attestation-0`.
///
/// Returns the client type prefix (everything before the last `-`).
pub fn client_type_from_id(client_id: &str) -> Option<&str> {
    client_id.rsplit_once('-').map(|(prefix, _)| prefix)
}

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
