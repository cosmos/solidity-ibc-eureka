use crate::types::ConsensusState;
use anchor_lang::prelude::*;

pub const CHUNK_DATA_SIZE: usize = 700;

#[account]
#[derive(InitSpace)]
pub struct ConsensusStateStore {
    pub height: u64,
    pub consensus_state: ConsensusState,
}

impl ConsensusStateStore {
    pub const SEED: &'static [u8] = solana_ibc_types::ConsensusState::SEED;
}

/// Storage for a single chunk of header data during multi-transaction upload
#[account]
#[derive(InitSpace)]
pub struct HeaderChunk {
    /// The chunk data
    #[max_len(CHUNK_DATA_SIZE)]
    pub chunk_data: Vec<u8>,
}

impl HeaderChunk {
    pub const SEED: &'static [u8] = b"header_chunk";
}

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
