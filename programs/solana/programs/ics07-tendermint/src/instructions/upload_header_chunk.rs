use crate::error::ErrorCode;
use crate::state::CHUNK_DATA_SIZE;
use crate::types::UploadChunkParams;
use crate::UploadHeaderChunk;
use anchor_lang::prelude::*;

pub fn upload_header_chunk(
    ctx: Context<UploadHeaderChunk>,
    params: UploadChunkParams,
) -> Result<()> {
    require!(
        !ctx.accounts.client_state.is_frozen(),
        ErrorCode::ClientFrozen
    );
    let chunk = &mut ctx.accounts.chunk;

    require!(
        params.chunk_data.len() <= CHUNK_DATA_SIZE,
        ErrorCode::ChunkDataTooLarge
    );

    chunk.chunk_data = params.chunk_data;

    Ok(())
}

#[cfg(test)]
mod tests;
