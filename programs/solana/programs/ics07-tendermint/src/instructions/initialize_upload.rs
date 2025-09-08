use crate::error::ErrorCode;
use crate::InitializeUpload;
use anchor_lang::prelude::*;

pub fn initialize_upload(
    ctx: Context<InitializeUpload>,
    chain_id: String,
    target_height: u64,
    total_chunks: u8,
    header_commitment: [u8; 32],
) -> Result<()> {
    let clock = Clock::get()?;
    let metadata = &mut ctx.accounts.metadata;

    require!(total_chunks > 0, ErrorCode::InvalidChunkCount);

    metadata.chain_id = chain_id;
    metadata.target_height = target_height;
    metadata.total_chunks = total_chunks;
    metadata.header_commitment = header_commitment;
    metadata.created_at = clock.unix_timestamp;
    metadata.updated_at = clock.unix_timestamp;

    Ok(())
}

