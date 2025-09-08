use crate::error::ErrorCode;
use crate::state::CHUNK_DATA_SIZE;
use crate::types::UploadChunkParams;
use crate::UploadHeaderChunk;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::keccak;

pub fn upload_header_chunk(
    ctx: Context<UploadHeaderChunk>,
    params: UploadChunkParams,
) -> Result<()> {
    let clock = Clock::get()?;
    let chunk = &mut ctx.accounts.chunk;
    let metadata = &mut ctx.accounts.metadata;

    require!(
        params.chunk_data.len() <= CHUNK_DATA_SIZE,
        ErrorCode::ChunkDataTooLarge
    );

    // Verify the provided hash matches the actual chunk data
    let computed_hash = keccak::hash(&params.chunk_data).0;
    require!(
        params.chunk_hash == computed_hash,
        ErrorCode::InvalidChunkHash
    );

    require!(
        metadata.chain_id == params.chain_id,
        ErrorCode::InvalidChunkAccount
    );
    require!(
        metadata.target_height == params.target_height,
        ErrorCode::InvalidChunkAccount
    );
    require!(
        metadata.total_chunks == params.total_chunks,
        ErrorCode::InvalidChunkCount
    );
    require!(
        metadata.header_commitment == params.header_commitment,
        ErrorCode::InvalidHeader
    );
    require!(
        params.chunk_index < metadata.total_chunks,
        ErrorCode::InvalidChunkIndex
    );

    metadata.updated_at = clock.unix_timestamp;

    chunk.chain_id = params.chain_id;
    chunk.target_height = params.target_height;
    chunk.chunk_index = params.chunk_index;
    chunk.chunk_hash = params.chunk_hash;
    chunk.chunk_data = params.chunk_data;
    chunk.version = chunk.version.wrapping_add(1); // Increment version on overwrites

    Ok(())
}

#[cfg(test)]
mod tests;
