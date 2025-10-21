use crate::constants::{CONSENSUS_STATE_PRUNING_GRACE_PERIOD, MAX_PRUNE_BATCH_SIZE};
use crate::error::ErrorCode;
use crate::state::ConsensusStateStore;
use crate::PruneConsensusStates;
use anchor_lang::prelude::*;

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct PruneConsensusStatesMsg {
    /// Heights to prune (must be below client_state.earliest_height)
    pub heights_to_prune: Vec<u64>,
}

pub fn prune_consensus_states<'info>(
    ctx: Context<'_, '_, '_, 'info, PruneConsensusStates<'info>>,
    msg: PruneConsensusStatesMsg,
) -> Result<()> {
    let client_state = &mut ctx.accounts.client_state;
    let current_time = Clock::get()?.unix_timestamp as u64;

    // Validate batch size
    require!(
        msg.heights_to_prune.len() <= MAX_PRUNE_BATCH_SIZE as usize,
        ErrorCode::ExceedsMaxBatchSize
    );

    require!(!msg.heights_to_prune.is_empty(), ErrorCode::EmptyBatch);

    let mut pruned_count: u16 = 0;

    // Process each consensus state to prune
    for (idx, &height_to_prune) in msg.heights_to_prune.iter().enumerate() {
        // Verify the height is below the earliest_height threshold
        require!(
            height_to_prune < client_state.earliest_height,
            ErrorCode::HeightNotPrunable
        );

        // Get the consensus state account from remaining_accounts
        let consensus_state_account = ctx
            .remaining_accounts
            .get(idx)
            .ok_or(ErrorCode::MissingAccount)?;

        // Verify this is the correct consensus state PDA
        let (expected_pda, _) = Pubkey::find_program_address(
            &[
                b"consensus_state",
                client_state.key().as_ref(),
                &height_to_prune.to_le_bytes(),
            ],
            ctx.program_id,
        );

        require!(
            expected_pda == consensus_state_account.key(),
            ErrorCode::InvalidAccount
        );

        // Load and verify the consensus state
        let data = consensus_state_account.try_borrow_data()?;
        if data.is_empty() {
            continue; // Already pruned, skip
        }

        // Deserialize to verify it's a valid consensus state
        let consensus_state_store: ConsensusStateStore =
            ConsensusStateStore::try_deserialize(&mut &data[..])
                .map_err(|_| error!(ErrorCode::SerializationError))?;

        // Verify the height matches
        require!(
            consensus_state_store.height == height_to_prune,
            ErrorCode::HeightMismatch
        );

        // Check grace period (optional - can be removed if immediate pruning is desired)
        // This gives time for any pending IBC packets to be processed
        let age = current_time.saturating_sub(consensus_state_store.consensus_state.timestamp);
        require!(
            age > CONSENSUS_STATE_PRUNING_GRACE_PERIOD,
            ErrorCode::PruningGracePeriodNotMet
        );

        // Close the account and reclaim rent
        let lamports = consensus_state_account.lamports();
        **consensus_state_account.try_borrow_mut_lamports()? = 0;
        **ctx.accounts.pruner.try_borrow_mut_lamports()? += lamports;

        // Clear the data to mark as closed
        consensus_state_account.try_borrow_mut_data()?.fill(0);

        pruned_count += 1;
    }

    // Update the consensus state count
    client_state.consensus_state_count = client_state
        .consensus_state_count
        .saturating_sub(pruned_count);

    msg!(
        "Pruned {} consensus states, new count: {}",
        pruned_count,
        client_state.consensus_state_count
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_prune_consensus_states_validation() {
        // Test cases will be added
    }
}