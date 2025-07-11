use crate::error::ErrorCode;
use crate::GetConsensusStateHash;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::keccak::hash as keccak256;

pub fn get_consensus_state_hash(
    ctx: Context<GetConsensusStateHash>,
    revision_height: u64,
) -> Result<[u8; 32]> {
    let consensus_state_store = &ctx.accounts.consensus_state_store;

    require!(
        consensus_state_store.height == revision_height,
        ErrorCode::HeightMismatch
    );

    let mut data = [0u8; 72];

    // Timestamp (8 bytes)
    data[0..8].copy_from_slice(
        &consensus_state_store
            .consensus_state
            .timestamp
            .to_le_bytes(),
    );

    // Root (32 bytes)
    data[8..40].copy_from_slice(&consensus_state_store.consensus_state.root);

    // Next validators hash (32 bytes)
    data[40..72].copy_from_slice(&consensus_state_store.consensus_state.next_validators_hash);

    Ok(keccak256(&data).to_bytes())
}

