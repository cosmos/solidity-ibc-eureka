use crate::error::ErrorCode;
use crate::state::CHUNK_DATA_SIZE;
use crate::types::UploadChunkParams;
use crate::UploadHeaderChunk;
use anchor_lang::prelude::*;

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

    require!(
        metadata.chain_id == params.chain_id,
        ErrorCode::InvalidChunkAccount
    );
    require!(
        metadata.target_height == params.target_height,
        ErrorCode::InvalidChunkAccount
    );
    require!(
        params.chunk_index < metadata.total_chunks,
        ErrorCode::InvalidChunkIndex
    );

    metadata.updated_at = clock.unix_timestamp;

    chunk.chain_id = params.chain_id;
    chunk.target_height = params.target_height;
    chunk.chunk_index = params.chunk_index;
    chunk.chunk_data = params.chunk_data;

    Ok(())
}

#[cfg(test)]
mod tests;
