use crate::error::ErrorCode;
use crate::UploadHeaderChunk;
use anchor_lang::prelude::*;

pub fn upload_header_chunk(
    ctx: Context<UploadHeaderChunk>,
    chain_id: String,
    target_height: u64,
    chunk_index: u8,
    total_chunks: u8,
    chunk_data: Vec<u8>,
    header_commitment: [u8; 32],
) -> Result<()> {
    let clock = Clock::get()?;
    let chunk = &mut ctx.accounts.chunk;
    let metadata = &mut ctx.accounts.metadata;

    // Initialize or update metadata
    if metadata.chain_id.is_empty() {
        // First chunk for this height - initialize metadata
        metadata.chain_id = chain_id.clone();
        metadata.target_height = target_height;
        metadata.total_chunks = total_chunks;
        metadata.header_commitment = header_commitment;
        metadata.created_at = clock.unix_timestamp;
        metadata.updated_at = clock.unix_timestamp;
    } else {
        // Validate metadata matches
        require_eq!(&metadata.chain_id, &chain_id);
        require_eq!(metadata.target_height, target_height);
        require_eq!(metadata.total_chunks, total_chunks);
        require!(
            metadata.header_commitment == header_commitment,
            ErrorCode::InvalidHeader
        );
        metadata.updated_at = clock.unix_timestamp;
    }

    // Store chunk data (overwrites if already exists)
    chunk.chain_id = chain_id;
    chunk.target_height = target_height;
    chunk.chunk_index = chunk_index;
    chunk.chunk_data = chunk_data;
    chunk.submitter = ctx.accounts.payer.key();
    chunk.version = chunk.version.wrapping_add(1); // Increment version on overwrites

    msg!(
        "Uploaded chunk {} of {} for height {}",
        chunk_index,
        total_chunks,
        target_height
    );
    Ok(())
}