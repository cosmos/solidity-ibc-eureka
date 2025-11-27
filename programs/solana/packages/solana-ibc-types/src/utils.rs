//! Utility functions for IBC on Solana

/// Compute Anchor instruction discriminator
///
/// This computes the first 8 bytes of SHA256("global:{instruction_name}")
/// following Anchor's discriminator calculation formula.
pub fn compute_discriminator(instruction_name: &str) -> [u8; 8] {
    let preimage = format!("global:{instruction_name}");
    let mut hash_result = [0u8; 8];
    hash_result.copy_from_slice(&solana_sha256_hasher::hash(preimage.as_bytes()).to_bytes()[..8]);
    hash_result
}
