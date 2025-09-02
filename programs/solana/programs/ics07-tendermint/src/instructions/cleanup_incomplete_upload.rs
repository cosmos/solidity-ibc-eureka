use crate::CleanupIncompleteUpload;
use anchor_lang::prelude::*;

pub fn cleanup_incomplete_upload(
    ctx: Context<CleanupIncompleteUpload>,
    chain_id: String,
    cleanup_height: u64,
) -> Result<()> {
    let metadata = &ctx.accounts.metadata;

    // Validate metadata
    require_eq!(&metadata.chain_id, &chain_id);
    require_eq!(metadata.target_height, cleanup_height);

    // Since we removed chunks_count, we need to check if all chunks exist
    // by attempting to verify the commitment (which would fail if chunks are missing)
    // For cleanup, we just close whatever chunks exist

    // Close all chunk accounts that were uploaded
    let mut closed_count = 0u8;
    for chunk_account in ctx.remaining_accounts.iter() {
        // Only close if it has lamports (i.e., it exists)
        let lamports = chunk_account.try_borrow_lamports()?;
        if **lamports > 0 {
            drop(lamports);
            let mut lamports = chunk_account.try_borrow_mut_lamports()?;
            let mut payer_lamports = ctx.accounts.payer.try_borrow_mut_lamports()?;
            **payer_lamports += **lamports;
            **lamports = 0;
            closed_count += 1;
        }
    }

    // Metadata account will be closed automatically by Anchor due to close = payer

    msg!(
        "Cleaned up incomplete upload at height {} ({} chunks closed)",
        cleanup_height,
        closed_count
    );
    Ok(())
}