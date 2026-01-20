//! Utility functions for IBC on Solana

use solana_ibc_constants::ANCHOR_DISCRIMINATOR_LEN;

/// Compute Anchor instruction discriminator
///
/// This computes the first 8 bytes of SHA256("global:{instruction_name}")
/// following Anchor's discriminator calculation formula.
pub fn compute_discriminator(instruction_name: &str) -> [u8; ANCHOR_DISCRIMINATOR_LEN] {
    let preimage = format!("global:{instruction_name}");
    let mut hash_result = [0u8; ANCHOR_DISCRIMINATOR_LEN];
    hash_result.copy_from_slice(
        &solana_sha256_hasher::hash(preimage.as_bytes()).to_bytes()[..ANCHOR_DISCRIMINATOR_LEN],
    );
    hash_result
}
