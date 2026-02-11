use crate::types::ClientState;
use anchor_lang::prelude::*;

#[derive(Accounts)]
#[instruction(client_id: String, submitter: Pubkey)]
pub struct CleanupIncompleteMisbehaviour<'info> {
    #[account(
        constraint = client_state.chain_id == client_id,
    )]
    pub client_state: Account<'info, ClientState>,

    #[account(
        mut,
        constraint = submitter_account.key() == submitter
    )]
    pub submitter_account: Signer<'info>,
    // Remaining accounts are the chunk accounts to close
}

pub fn cleanup_incomplete_misbehaviour(
    ctx: Context<CleanupIncompleteMisbehaviour>,
    client_id: String,
    submitter: Pubkey,
) -> Result<()> {
    for (index, chunk_account) in ctx.remaining_accounts.iter().enumerate() {
        let expected_seeds = &[
            crate::state::MisbehaviourChunk::SEED,
            submitter.as_ref(),
            client_id.as_bytes(),
            &[index as u8],
        ];
        let (expected_chunk_pda, _) = Pubkey::find_program_address(expected_seeds, &crate::ID);

        if chunk_account.key() != expected_chunk_pda {
            continue;
        }

        if chunk_account.owner == &crate::ID && chunk_account.lamports() > 0 {
            {
                let mut data = chunk_account.try_borrow_mut_data()?;
                data.fill(0);
            }

            let mut lamports = chunk_account.try_borrow_mut_lamports()?;
            let mut submitter_lamports =
                ctx.accounts.submitter_account.try_borrow_mut_lamports()?;
            **submitter_lamports = submitter_lamports
                .checked_add(**lamports)
                .ok_or(crate::error::ErrorCode::ArithmeticOverflow)?;
            **lamports = 0;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests;
