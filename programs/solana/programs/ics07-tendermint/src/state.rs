use crate::types::ConsensusState;
use anchor_lang::prelude::*;
pub use solana_ibc_constants::CHUNK_DATA_SIZE;

#[account]
#[derive(InitSpace)]
pub struct ConsensusStateStore {
    pub height: u64,
    pub consensus_state: ConsensusState,
}

impl ConsensusStateStore {
    pub const SEED: &'static [u8] = solana_ibc_types::ConsensusState::SEED;

    /// Returns `true` when `init_if_needed` just created the account
    /// (Tendermint heights start at 1, so zero means uninitialized).
    pub const fn is_uninitialized(&self) -> bool {
        self.height == 0
    }
}

/// Storage for a single chunk of header data during multi-transaction upload
#[account]
#[derive(InitSpace)]
pub struct HeaderChunk {
    /// The submitter who created this chunk
    pub submitter: Pubkey,
    /// The chunk data
    #[max_len(CHUNK_DATA_SIZE)]
    pub chunk_data: Vec<u8>,
}

impl HeaderChunk {
    pub const SEED: &'static [u8] = b"header_chunk";
}

/// Storage for a single chunk of misbehaviour data during multi-transaction upload
#[account]
#[derive(InitSpace)]
pub struct MisbehaviourChunk {
    /// The chunk data
    #[max_len(CHUNK_DATA_SIZE)]
    pub chunk_data: Vec<u8>,
}

impl MisbehaviourChunk {
    pub const SEED: &'static [u8] = b"misbehaviour_chunk";
}

/// Storage for Ed25519 signature verification results.
/// IMPORTANT: Field order matters: verifier reads `data[8]` for `is_valid`.
#[account]
#[derive(InitSpace)]
pub struct SignatureVerification {
    /// Whether the signature is valid
    pub is_valid: bool,
    /// The submitter who created this verification
    pub submitter: Pubkey,
}

impl SignatureVerification {
    pub const SEED: &'static [u8] = b"sig_verify";
}

// Compile-time verification that discriminator length matches the shared constant
const _: () = assert!(
    SignatureVerification::DISCRIMINATOR.len() == solana_ibc_constants::ANCHOR_DISCRIMINATOR_LEN
);

#[cfg(test)]
mod compatibility_tests {
    use super::*;

    /// Ensures `ConsensusStateStore` SEED constant matches solana-ibc-types
    #[test]
    fn test_consensus_state_store_seed_compatibility() {
        assert_eq!(
            ConsensusStateStore::SEED,
            solana_ibc_types::ConsensusState::SEED
        );
    }
}
