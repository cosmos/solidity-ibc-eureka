use crate::error::ErrorCode;
use crate::state::{HeaderChunk, CHUNK_DATA_SIZE};
use crate::types::{ClientState, UploadChunkParams};
use anchor_lang::prelude::*;

#[derive(Accounts)]
#[instruction(params: UploadChunkParams)]
pub struct UploadHeaderChunk<'info> {
    /// The header chunk account to create (fails if already exists)
    #[account(
        init,
        payer = submitter,
        space = 8 + HeaderChunk::INIT_SPACE,
        seeds = [
            HeaderChunk::SEED,
            submitter.key().as_ref(),
            params.chain_id.as_bytes(),
            &params.target_height.to_le_bytes(),
            &[params.chunk_index]
        ],
        bump
    )]
    pub chunk: Account<'info, HeaderChunk>,

    /// Client state to verify this is a valid client
    #[account(
        constraint = client_state.chain_id == params.chain_id,
    )]
    pub client_state: Account<'info, ClientState>,

    /// The submitter who pays for and owns these accounts
    #[account(mut)]
    pub submitter: Signer<'info>,
    pub system_program: Program<'info, System>,
}

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

    chunk.submitter = ctx.accounts.submitter.key();
    chunk.chunk_data = params.chunk_data;

    Ok(())
}

#[cfg(test)]
mod tests;
