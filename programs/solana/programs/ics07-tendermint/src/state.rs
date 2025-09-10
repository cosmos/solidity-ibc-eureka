use crate::types::ConsensusState;
use anchor_lang::prelude::*;

pub const CHUNK_DATA_SIZE: usize = 700;

#[account]
#[derive(InitSpace)]
pub struct ConsensusStateStore {
    pub height: u64,
    pub consensus_state: ConsensusState,
}

/// Storage for a single chunk of header data during multi-transaction upload
#[account]
#[derive(InitSpace)]
pub struct HeaderChunk {
    /// Chain ID this chunk belongs to
    #[max_len(32)]
    pub chain_id: String,
    /// Target height for this header
    pub target_height: u64,
    /// Index of this chunk (0-based)
    pub chunk_index: u8,
    /// The chunk data
    #[max_len(CHUNK_DATA_SIZE)]
    pub chunk_data: Vec<u8>,
}

/// Metadata for tracking header upload by height
#[account]
#[derive(InitSpace)]
pub struct HeaderMetadata {
    /// Chain ID this header is for
    #[max_len(32)]
    pub chain_id: String,
    /// Target height for this update
    pub target_height: u64,
    /// Expected total chunks
    pub total_chunks: u8,
    /// Commitment to the complete header (hash of all chunks concatenated)
    pub header_commitment: [u8; 32],
    // Track when upload started (optional, will be used to track perf)
    pub created_at: i64,
    // Track last activity (optional, will be used to track perf)
    pub updated_at: i64,
}
