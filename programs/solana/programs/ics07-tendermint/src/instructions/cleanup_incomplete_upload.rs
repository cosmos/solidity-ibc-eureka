use crate::CleanupIncompleteUpload;
use anchor_lang::prelude::*;

pub fn cleanup_incomplete_upload(
    ctx: Context<CleanupIncompleteUpload>,
    chain_id: String,
    cleanup_height: u64,
    submitter: Pubkey,
) -> Result<()> {
    let metadata = &ctx.accounts.metadata;

    // Validate metadata
    require_eq!(&metadata.chain_id, &chain_id);
    require_eq!(metadata.target_height, cleanup_height);

    // Since we dont have uploaded chunks_count, we need to check if all chunks exist
    // by attempting to verify the commitment (which would fail if chunks are missing)
    // For cleanup, we just close whatever chunks exist

    // Close all chunk accounts that were uploaded
    // IMPORTANT: We must validate that these are actually chunk PDAs to avoid
    // accidentally closing other accounts
    let mut closed_count = 0u8;
    for (index, chunk_account) in ctx.remaining_accounts.iter().enumerate() {
        // Derive the expected chunk PDA for this index
        let expected_seeds = &[
            b"header_chunk".as_ref(),
            submitter.as_ref(),
            chain_id.as_bytes(),
            &cleanup_height.to_le_bytes(),
            &[index as u8],
        ];
        let (expected_chunk_pda, _) = Pubkey::find_program_address(expected_seeds, ctx.program_id);

        // CRITICAL: Verify this is the correct chunk account
        if chunk_account.key() != expected_chunk_pda {
            // This is not the chunk we're looking for, skip it
            // This could happen if the relayer passes accounts in wrong order
            continue;
        }

        // Check if account exists and is owned by our program
        if chunk_account.owner == ctx.program_id && chunk_account.lamports() > 0 {
            // Safe to close - it's a verified chunk PDA owned by our program
            let mut lamports = chunk_account.try_borrow_mut_lamports()?;
            let mut submitter_lamports =
                ctx.accounts.submitter_account.try_borrow_mut_lamports()?;
            **submitter_lamports += **lamports;
            **lamports = 0;
            closed_count += 1;
        }
        // If account doesn't exist or isn't owned by us, skip it
    }

    // Metadata account will be closed automatically by Anchor due to close = submitter_account

    msg!(
        "Cleaned up incomplete upload at height {} ({} chunks closed)",
        cleanup_height,
        closed_count
    );
    Ok(())
}

#[cfg(test)]
#[path = "cleanup_incomplete_upload_test.rs"]
mod cleanup_incomplete_upload_test;
