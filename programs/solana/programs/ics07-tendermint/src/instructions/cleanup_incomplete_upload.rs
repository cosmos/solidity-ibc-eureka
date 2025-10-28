use crate::CleanupIncompleteUpload;
use anchor_lang::prelude::*;

pub fn cleanup_incomplete_upload(
    ctx: Context<CleanupIncompleteUpload>,
    chain_id: String,
    cleanup_height: u64,
    submitter: Pubkey,
) -> Result<()> {
    // Close all chunk accounts that were uploaded
    // IMPORTANT: We must validate that these are actually chunk PDAs to avoid
    // accidentally closing other accounts
    for (index, chunk_account) in ctx.remaining_accounts.iter().enumerate() {
        // Derive the expected chunk PDA for this index
        let expected_seeds = &[
            crate::state::HeaderChunk::SEED,
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
        }
        // If account doesn't exist or isn't owned by us, skip it
    }

    Ok(())
}

#[cfg(test)]
mod tests;
