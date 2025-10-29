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
        // Try to deserialize to get the height
        let account_data = consensus_account.try_borrow_data()?;
        if account_data.is_empty() {
            continue; // Skip empty accounts
        }

        let consensus_store: crate::state::ConsensusStateStore =
            crate::state::ConsensusStateStore::try_deserialize(&mut &account_data[..])?;
        drop(account_data); // Release borrow before modifying lamports

        let height = consensus_store.height;

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

        // Close the account and transfer rent to receiver
        if consensus_account.owner == ctx.program_id && consensus_account.lamports() > 0 {
            let mut lamports = consensus_account.try_borrow_mut_lamports()?;
            let mut receiver_lamports = ctx.accounts.rent_receiver.try_borrow_mut_lamports()?;
            **receiver_lamports += **lamports;
            **lamports = 0;

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
