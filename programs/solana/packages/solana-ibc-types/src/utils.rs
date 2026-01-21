//! Utility functions for IBC on Solana

use solana_ibc_constants::ANCHOR_DISCRIMINATOR_LEN;
use solana_sha256_hasher::{hash as sha256, hashv};

/// Compute Anchor instruction discriminator
///
/// This computes the first 8 bytes of SHA256("global:{instruction_name}")
/// following Anchor's discriminator calculation formula.
pub fn compute_discriminator(instruction_name: &str) -> [u8; ANCHOR_DISCRIMINATOR_LEN] {
    let preimage = format!("global:{instruction_name}");
    let mut hash_result = [0u8; ANCHOR_DISCRIMINATOR_LEN];
    hash_result
        .copy_from_slice(&sha256(preimage.as_bytes()).to_bytes()[..ANCHOR_DISCRIMINATOR_LEN]);
    hash_result
}

/// IBC commitment version byte.
const IBC_VERSION: u8 = 0x02;

/// Compute IBC acknowledgement commitment for a single acknowledgement.
///
/// IBC commitment format: `sha256(0x02 || sha256(ack))`
/// where 0x02 is the IBC version byte.
pub fn ibc_ack_commitment(ack: &[u8]) -> [u8; 32] {
    let ack_hash = sha256(ack).to_bytes();
    hashv(&[&[IBC_VERSION], &ack_hash]).to_bytes()
}
