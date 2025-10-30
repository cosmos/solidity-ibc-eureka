use crate::error::ErrorCode;
use crate::PruneConsensusStates;
use anchor_lang::prelude::*;

pub fn prune_consensus_states(ctx: Context<PruneConsensusStates>, _chain_id: String) -> Result<()> {
    let client_state = &mut ctx.accounts.client_state;
    let client_key = client_state.key();

    // Track which heights we successfully pruned
    let mut pruned_heights = Vec::new();

    // Iterate through remaining accounts (consensus state accounts to prune)
    for consensus_account in ctx.remaining_accounts {
        // Skip accounts not owned by our program (e.g., payer accounts)
        if consensus_account.owner != ctx.program_id {
            continue;
        }

        // Skip accounts with no lamports
        if consensus_account.lamports() == 0 {
            continue;
        }

        // Try to deserialize to get the height and payer
        let account_data = consensus_account.try_borrow_data()?;
        if account_data.is_empty() {
            continue; // Skip empty accounts
        }

        let consensus_store: crate::state::ConsensusStateStore =
            crate::state::ConsensusStateStore::try_deserialize(&mut &account_data[..])?;
        let height = consensus_store.height;
        let original_payer = consensus_store.payer;
        drop(account_data); // Release borrow before modifying lamports

        // CRITICAL: Only allow pruning if height is in the to_prune list
        if !client_state
            .consensus_state_heights_to_prune
            .contains(&height)
        {
            continue;
        }

        // Verify PDA is correct for this height
        let (expected_pda, _) = Pubkey::find_program_address(
            &[
                crate::state::ConsensusStateStore::SEED,
                client_key.as_ref(),
                &height.to_le_bytes(),
            ],
            ctx.program_id,
        );

        require_eq!(
            consensus_account.key(),
            expected_pda,
            ErrorCode::AccountValidationFailed
        );

        // Close the account and distribute rent with bounty system
        {
            let total_lamports = consensus_account.lamports();

            let mut lamports = consensus_account.try_borrow_mut_lamports()?;

            // Check if payer and pruner are the same account
            if original_payer == ctx.accounts.rent_receiver.key() {
                // Same account: give full rent directly to rent_receiver
                // NOTE: Payer should NOT be in remaining_accounts in this case to avoid duplicate writable account
                let mut receiver_lamports = ctx.accounts.rent_receiver.try_borrow_mut_lamports()?;

                // Transfer lamports: subtract from source, add to destination
                **lamports = lamports
                    .checked_sub(total_lamports)
                    .ok_or(ErrorCode::ArithmeticError)?;
                **receiver_lamports = receiver_lamports
                    .checked_add(total_lamports)
                    .ok_or(ErrorCode::ArithmeticError)?;
            } else {
                // Calculate splits: 95% to original payer, 5% to pruner
                let pruner_bounty = total_lamports
                    .checked_mul(5)
                    .and_then(|v| v.checked_div(100))
                    .ok_or(ErrorCode::ArithmeticError)?;
                let payer_refund = total_lamports
                    .checked_sub(pruner_bounty)
                    .ok_or(ErrorCode::ArithmeticError)?;

                // Find original payer account in remaining accounts - REQUIRED when different from pruner
                let payer_account = ctx
                    .remaining_accounts
                    .iter()
                    .find(|acc| acc.key() == original_payer)
                    .ok_or(ErrorCode::MissingAccount)?;

                // Different accounts: split rent (95% to payer, 5% to pruner)
                let mut payer_lamports = payer_account.try_borrow_mut_lamports()?;
                let mut pruner_lamports = ctx.accounts.rent_receiver.try_borrow_mut_lamports()?;

                // Transfer lamports: subtract from source first
                **lamports = lamports
                    .checked_sub(total_lamports)
                    .ok_or(ErrorCode::ArithmeticError)?;

                // Then add to destinations
                **payer_lamports = payer_lamports
                    .checked_add(payer_refund)
                    .ok_or(ErrorCode::ArithmeticError)?;
                **pruner_lamports = pruner_lamports
                    .checked_add(pruner_bounty)
                    .ok_or(ErrorCode::ArithmeticError)?;
            }
            pruned_heights.push(height);
        }
    }

    // Remove successfully pruned heights from the to_prune list
    client_state
        .consensus_state_heights_to_prune
        .retain(|h| !pruned_heights.contains(h));

    Ok(())
}

#[cfg(test)]
mod tests;
