//! Constants and program IDs for IBC on Solana
//!
//! This crate provides all the program IDs and constants used by IBC on Solana

/// Universal error acknowledgement as defined in ICS-04.
pub const UNIVERSAL_ERROR_ACK: [u8; 32] = [
    0x47, 0x74, 0xd4, 0xa5, 0x75, 0x99, 0x3f, 0x96, 0x3b, 0x1c, 0x06, 0x57, 0x37, 0x36, 0x61, 0x7a,
    0x45, 0x7a, 0xbe, 0xf8, 0x58, 0x91, 0x78, 0xdb, 0x8d, 0x10, 0xc9, 0x4b, 0x4a, 0xb5, 0x11, 0xab,
];

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
