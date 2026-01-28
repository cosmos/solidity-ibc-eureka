use crate::error::ErrorCode;
use crate::state::CHUNK_DATA_SIZE;
use crate::types::UploadMisbehaviourChunkParams;
use crate::UploadMisbehaviourChunk;
use anchor_lang::prelude::*;

pub fn upload_misbehaviour_chunk(
    ctx: Context<UploadMisbehaviourChunk>,
    params: UploadMisbehaviourChunkParams,
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

#[cfg_attr(coverage_nightly, coverage(off))]
#[cfg(test)]
mod tests;
