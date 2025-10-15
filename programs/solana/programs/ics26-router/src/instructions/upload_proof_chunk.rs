use crate::errors::RouterError;
use crate::state::*;
use anchor_lang::prelude::*;

#[derive(Accounts)]
#[instruction(msg: MsgUploadChunk)]
pub struct UploadProofChunk<'info> {
    #[account(
        init,
        payer = payer,
        space = 8 + ProofChunk::INIT_SPACE,
        seeds = [
            PROOF_CHUNK_SEED,
            payer.key().as_ref(),
            msg.client_id.as_bytes(),
            &msg.sequence.to_le_bytes(),
            &[msg.chunk_index]
        ],
        bump
    )]
    pub chunk: Account<'info, ProofChunk>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn upload_proof_chunk(ctx: Context<UploadProofChunk>, msg: MsgUploadChunk) -> Result<()> {
    let chunk = &mut ctx.accounts.chunk;

    require!(
        msg.chunk_data.len() <= CHUNK_DATA_SIZE,
        RouterError::ChunkDataTooLarge
    );

    chunk.client_id = msg.client_id;
    chunk.sequence = msg.sequence;
    chunk.chunk_index = msg.chunk_index;
    chunk.chunk_data = msg.chunk_data;

    Ok(())
}
