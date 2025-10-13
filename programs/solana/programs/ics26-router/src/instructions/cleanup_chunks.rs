use crate::errors::RouterError;
use crate::state::*;
use anchor_lang::prelude::*;

#[derive(Accounts)]
#[instruction(msg: MsgCleanupChunks)]
pub struct CleanupChunks<'info> {
    /// Relayer who created the chunks and can clean them up
    #[account(mut)]
    pub relayer: Signer<'info>,
}

pub fn cleanup_chunks<'info>(
    ctx: Context<'_, '_, '_, 'info, CleanupChunks<'info>>,
    msg: MsgCleanupChunks,
) -> Result<()> {
    let relayer_key = ctx.accounts.relayer.key();
    let mut chunk_index = 0;

    // Clean payload chunks for each payload
    for (payload_idx, &total_chunks) in msg.payload_chunks.iter().enumerate() {
        for i in 0..total_chunks {
            require!(
                chunk_index < ctx.remaining_accounts.len(),
                RouterError::InvalidChunkCount
            );

            let chunk_account = &ctx.remaining_accounts[chunk_index];

            // Verify the PDA is correct
            let expected_seeds = &[
                PAYLOAD_CHUNK_SEED,
                relayer_key.as_ref(),
                msg.client_id.as_bytes(),
                &msg.sequence.to_le_bytes(),
                &[payload_idx as u8],
                &[i],
            ];
            let (expected_pda, _) = Pubkey::find_program_address(expected_seeds, ctx.program_id);

            require!(
                chunk_account.key() == expected_pda,
                RouterError::InvalidChunkAccount
            );

            // Return rent to relayer
            cleanup_single_chunk(chunk_account, &ctx.accounts.relayer)?;
            chunk_index += 1;
        }
    }

    // Clean proof chunks
    for i in 0..msg.total_proof_chunks {
        require!(
            chunk_index < ctx.remaining_accounts.len(),
            RouterError::InvalidChunkCount
        );

        let chunk_account = &ctx.remaining_accounts[chunk_index];

        // Verify the PDA is correct
        let expected_seeds = &[
            PROOF_CHUNK_SEED,
            relayer_key.as_ref(),
            msg.client_id.as_bytes(),
            &msg.sequence.to_le_bytes(),
            &[i],
        ];
        let (expected_pda, _) = Pubkey::find_program_address(expected_seeds, ctx.program_id);

        require!(
            chunk_account.key() == expected_pda,
            RouterError::InvalidChunkAccount
        );

        // Return rent to relayer
        cleanup_single_chunk(chunk_account, &ctx.accounts.relayer)?;
        chunk_index += 1;
    }

    Ok(())
}

fn cleanup_single_chunk<'info>(
    chunk_account: &AccountInfo<'info>,
    relayer: &Signer<'info>,
) -> Result<()> {
    let mut chunk_lamports = chunk_account.try_borrow_mut_lamports()?;
    let mut relayer_lamports = relayer.try_borrow_mut_lamports()?;

    **relayer_lamports = relayer_lamports
        .checked_add(**chunk_lamports)
        .ok_or(RouterError::ArithmeticOverflow)?;
    **chunk_lamports = 0;

    let mut data = chunk_account.try_borrow_mut_data()?;
    data.fill(0);

    Ok(())
}

