use crate::error::ErrorCode;
use crate::state::{MisbehaviourChunk, CHUNK_DATA_SIZE};
use crate::types::{ClientState, UploadMisbehaviourChunkParams};
use anchor_lang::prelude::*;

#[derive(Accounts)]
#[instruction(params: UploadMisbehaviourChunkParams)]
pub struct UploadMisbehaviourChunk<'info> {
    #[account(
        init,
        payer = submitter,
        space = 8 + MisbehaviourChunk::INIT_SPACE,
        seeds = [
            MisbehaviourChunk::SEED,
            submitter.key().as_ref(),
            &[params.chunk_index]
        ],
        bump
    )]
    pub chunk: Account<'info, MisbehaviourChunk>,

    #[account(
        seeds = [ClientState::SEED],
        bump
    )]
    pub client_state: Account<'info, ClientState>,

    #[account(mut)]
    pub submitter: Signer<'info>,
    pub system_program: Program<'info, System>,
}

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

#[cfg(test)]
mod tests;
