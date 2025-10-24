use crate::constants::{CONSENSUS_STATE_PRUNING_GRACE_PERIOD, MAX_PRUNE_BATCH_SIZE};
use crate::error::ErrorCode;
use crate::state::ConsensusStateStore;
use crate::PruneConsensusStates;
use anchor_lang::prelude::*;

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct PruneConsensusStatesMsg {
    /// Heights to prune (must be below `client_state.earliest_height`)
    pub heights_to_prune: Vec<u64>,
}

/// Helper function to close a consensus state account and reclaim rent
fn close_consensus_state<'info>(
    consensus_state_account: &AccountInfo<'info>,
    pruner: &AccountInfo<'info>,
) -> Result<()> {
    let lamports_to_reclaim = consensus_state_account.lamports();

    // Transfer lamports to pruner
    **pruner.lamports.borrow_mut() = pruner
        .lamports()
        .checked_add(lamports_to_reclaim)
        .unwrap_or_else(|| pruner.lamports());
    **consensus_state_account.lamports.borrow_mut() = 0;

    // Clear account data
    let mut data = consensus_state_account.try_borrow_mut_data()?;
    data.fill(0);

    Ok(())
}

pub fn prune_consensus_states<'info>(
    ctx: Context<'_, '_, '_, 'info, PruneConsensusStates<'info>>,
    msg: PruneConsensusStatesMsg,
) -> Result<()> {
    let client_state = &mut ctx.accounts.client_state;

    // Get current time for grace period check
    // In tests, Clock::get() returns the mocked clock sysvar
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

        // Skip if already pruned (empty data or 0 lamports)
        if consensus_state_account.lamports() == 0 || consensus_state_account.data_is_empty() {
            continue;
        }

        // Deserialize and validate the consensus state
        // Scope the borrow to ensure it's dropped before close_account
        let (stored_height, consensus_state_timestamp) = {
            let data = consensus_state_account.try_borrow_data()?;

            // Deserialize ConsensusStateStore to get height and timestamp
            let mut data_slice = &data[8..]; // Skip 8-byte Anchor discriminator
            let store: ConsensusStateStore =
                anchor_lang::AnchorDeserialize::deserialize(&mut data_slice)
                    .map_err(|_| ErrorCode::InvalidAccount)?;

            (store.height, store.consensus_state.timestamp)
        }; // Borrow is dropped here

        // Verify height matches
        require!(stored_height == height_to_prune, ErrorCode::HeightMismatch);

        // Verify grace period has passed
        let time_since_consensus = current_time.saturating_sub(consensus_state_timestamp);
        require!(
            time_since_consensus >= CONSENSUS_STATE_PRUNING_GRACE_PERIOD,
            ErrorCode::PruningGracePeriodNotMet
        );

        // Close the account and reclaim rent
        let pruner_info = ctx.accounts.pruner.to_account_info();
        close_consensus_state(consensus_state_account, &pruner_info)?;

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
    use super::*;
    use crate::test_helpers::{chunk_test_utils::*, fixtures::assert_error_code};
    use anchor_lang::InstructionData;
    use mollusk_svm::result::Check;
    use mollusk_svm::Mollusk;
    use solana_sdk::instruction::{AccountMeta, Instruction};
    use solana_sdk::pubkey::Pubkey;

    fn create_consensus_state_for_pruning(
        client_state_key: Pubkey,
        height: u64,
        timestamp: u64,
    ) -> (Pubkey, solana_sdk::account::Account) {
        use crate::state::ConsensusStateStore;
        use anchor_lang::AccountSerialize;

        let (consensus_state_pda, _) = Pubkey::find_program_address(
            &[
                b"consensus_state",
                client_state_key.as_ref(),
                &height.to_le_bytes(),
            ],
            &crate::ID,
        );

        let consensus_state_store = ConsensusStateStore {
            height,
            consensus_state: crate::types::ConsensusState {
                timestamp,
                root: [0u8; 32],
                next_validators_hash: [1u8; 32],
            },
        };

        let mut data = vec![];
        consensus_state_store.try_serialize(&mut data).unwrap();

        let account = solana_sdk::account::Account {
            lamports: 1_500_000,
            data,
            owner: crate::ID,
            executable: false,
            rent_epoch: 0,
        };

        (consensus_state_pda, account)
    }

    fn create_client_state_account_with_pruning_config(
        chain_id: &str,
        latest_height: u64,
        earliest_height: u64,
        consensus_state_count: u16,
    ) -> solana_sdk::account::Account {
        use crate::types::{ClientState, IbcHeight};
        use anchor_lang::AccountSerialize;

        let client_state = ClientState {
            chain_id: chain_id.to_string(),
            trust_level_numerator: 2,
            trust_level_denominator: 3,
            trusting_period: 86400,
            unbonding_period: 172_800,
            max_clock_drift: 600,
            frozen_height: IbcHeight {
                revision_number: 0,
                revision_height: 0,
            },
            latest_height: IbcHeight {
                revision_number: 0,
                revision_height: latest_height,
            },
            earliest_height,
            consensus_state_count,
            max_consensus_states: 100,
        };

        let mut data = vec![];
        client_state.try_serialize(&mut data).unwrap();

        solana_sdk::account::Account {
            lamports: 1_000_000,
            data,
            owner: crate::ID,
            executable: false,
            rent_epoch: 0,
        }
    }

    fn create_clock_account(timestamp: i64) -> (Pubkey, solana_sdk::account::Account) {
        use solana_sdk::clock::Clock;
        use solana_sdk::sysvar;

        let clock = Clock {
            slot: 1000,
            epoch_start_timestamp: 0,
            epoch: 1,
            leader_schedule_epoch: 1,
            unix_timestamp: timestamp,
        };

        let data = bincode::serialize(&clock).expect("Failed to serialize Clock");

        (
            sysvar::clock::ID,
            solana_sdk::account::Account {
                lamports: 1,
                data,
                owner: sysvar::ID,
                executable: false,
                rent_epoch: 0,
            },
        )
    }

    #[test]
    fn test_prune_consensus_states_success() {
        use anchor_lang::AnchorDeserialize;

        let pruner = Pubkey::new_unique();
        let chain_id = "test-chain";

        // Create client state with proper pruning configuration
        let client_state_pda = derive_client_state_pda(chain_id);
        let client_state_account = create_client_state_account_with_pruning_config(
            chain_id, 15, // latest_height
            10, // earliest_height
            5,  // consensus_state_count - start with 5, prune 1, expect 4
        );

        // Create clock with current time = 100000 (past grace period)
        let current_time = 100_000i64;
        let (clock_pda, clock_account) = create_clock_account(current_time);

        // Create consensus state to prune (below earliest_height threshold)
        let old_timestamp =
            (current_time as u64).saturating_sub(CONSENSUS_STATE_PRUNING_GRACE_PERIOD + 1000);
        let (cs1_pda, cs1_account) =
            create_consensus_state_for_pruning(client_state_pda, 5, old_timestamp);

        let msg = PruneConsensusStatesMsg {
            heights_to_prune: vec![5],
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(client_state_pda, false),
                AccountMeta::new(pruner, true),
                AccountMeta::new(cs1_pda, false),
            ],
            data: crate::instruction::PruneConsensusStates { msg }.data(),
        };

        let accounts = vec![
            (client_state_pda, client_state_account),
            (pruner, create_submitter_account(10_000_000)),
            (cs1_pda, cs1_account),
            (clock_pda, clock_account),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::test_helpers::PROGRAM_BINARY_PATH);

        let checks = vec![
            Check::success(),
            // Verify consensus state was closed (0 lamports)
            Check::account(&cs1_pda).lamports(0).build(),
            // Verify pruner received rent (initial 10_000_000 + 1 * 1_500_000)
            Check::account(&pruner).lamports(11_500_000).build(),
        ];

        let result = mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);

        // Verify client_state.consensus_state_count was decremented
        let client_state_account = result
            .resulting_accounts
            .iter()
            .find(|(pubkey, _)| pubkey == &client_state_pda)
            .map(|(_, account)| account)
            .expect("Client state account not found");

        let mut data_slice = &client_state_account.data[8..]; // Skip 8-byte discriminator
        let client_state: crate::types::ClientState =
            crate::types::ClientState::deserialize(&mut data_slice).unwrap();

        assert_eq!(
            client_state.consensus_state_count, 4,
            "Should have decremented count by 1"
        );
    }

    #[test]
    fn test_prune_consensus_states_exceeds_max_batch_size() {
        let pruner = Pubkey::new_unique();
        let chain_id = "test-chain";

        let client_state_pda = derive_client_state_pda(chain_id);
        let client_state_account = create_client_state_account(chain_id, 15);

        // Try to prune more than MAX_PRUNE_BATCH_SIZE (5)
        let msg = PruneConsensusStatesMsg {
            heights_to_prune: vec![1, 2, 3, 4, 5, 6], // 6 heights > MAX_PRUNE_BATCH_SIZE
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(client_state_pda, false),
                AccountMeta::new(pruner, true),
            ],
            data: crate::instruction::PruneConsensusStates { msg }.data(),
        };

        let accounts = vec![
            (client_state_pda, client_state_account),
            (pruner, create_submitter_account(10_000_000)),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::test_helpers::PROGRAM_BINARY_PATH);
        let result = mollusk.process_instruction(&instruction, &accounts);

        assert_error_code(
            result,
            ErrorCode::ExceedsMaxBatchSize,
            "exceeds_max_batch_size",
        );
    }

    #[test]
    fn test_prune_consensus_states_empty_batch() {
        let pruner = Pubkey::new_unique();
        let chain_id = "test-chain";

        let client_state_pda = derive_client_state_pda(chain_id);
        let client_state_account = create_client_state_account(chain_id, 15);

        let msg = PruneConsensusStatesMsg {
            heights_to_prune: vec![], // Empty batch
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(client_state_pda, false),
                AccountMeta::new(pruner, true),
            ],
            data: crate::instruction::PruneConsensusStates { msg }.data(),
        };

        let accounts = vec![
            (client_state_pda, client_state_account),
            (pruner, create_submitter_account(10_000_000)),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::test_helpers::PROGRAM_BINARY_PATH);
        let result = mollusk.process_instruction(&instruction, &accounts);

        assert_error_code(result, ErrorCode::EmptyBatch, "empty_batch");
    }

    #[test]
    fn test_prune_consensus_states_height_not_prunable() {
        let pruner = Pubkey::new_unique();
        let chain_id = "test-chain";

        let client_state_pda = derive_client_state_pda(chain_id);
        let client_state_account = create_client_state_account_with_pruning_config(
            chain_id, 15, // latest_height
            10, // earliest_height
            1,  // consensus_state_count
        );

        // Try to prune height 10 (not < earliest_height)
        let msg = PruneConsensusStatesMsg {
            heights_to_prune: vec![10], // height >= earliest_height
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(client_state_pda, false),
                AccountMeta::new(pruner, true),
            ],
            data: crate::instruction::PruneConsensusStates { msg }.data(),
        };

        let accounts = vec![
            (client_state_pda, client_state_account),
            (pruner, create_submitter_account(10_000_000)),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::test_helpers::PROGRAM_BINARY_PATH);
        let result = mollusk.process_instruction(&instruction, &accounts);

        assert_error_code(result, ErrorCode::HeightNotPrunable, "height_not_prunable");
    }

    #[test]
    fn test_prune_consensus_states_missing_account() {
        let pruner = Pubkey::new_unique();
        let chain_id = "test-chain";

        let client_state_pda = derive_client_state_pda(chain_id);
        let client_state_account = create_client_state_account_with_pruning_config(
            chain_id, 15, // latest_height
            10, // earliest_height
            1,  // consensus_state_count
        );

        // Try to prune height 5 but don't provide the consensus state account
        let msg = PruneConsensusStatesMsg {
            heights_to_prune: vec![5],
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(client_state_pda, false),
                AccountMeta::new(pruner, true),
                // Missing consensus state account
            ],
            data: crate::instruction::PruneConsensusStates { msg }.data(),
        };

        let accounts = vec![
            (client_state_pda, client_state_account),
            (pruner, create_submitter_account(10_000_000)),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::test_helpers::PROGRAM_BINARY_PATH);
        let result = mollusk.process_instruction(&instruction, &accounts);

        assert_error_code(result, ErrorCode::MissingAccount, "missing_account");
    }

    #[test]
    fn test_prune_consensus_states_invalid_account() {
        let pruner = Pubkey::new_unique();
        let chain_id = "test-chain";

        let client_state_pda = derive_client_state_pda(chain_id);
        let client_state_account = create_client_state_account_with_pruning_config(
            chain_id, 15, // latest_height
            10, // earliest_height
            1,  // consensus_state_count
        );

        let current_time = 100_000i64;
        let (clock_pda, clock_account) = create_clock_account(current_time);

        // Create a consensus state with wrong pubkey
        let wrong_pubkey = Pubkey::new_unique();
        let old_timestamp =
            (current_time as u64).saturating_sub(CONSENSUS_STATE_PRUNING_GRACE_PERIOD + 1000);
        let (_, cs_account) =
            create_consensus_state_for_pruning(client_state_pda, 5, old_timestamp);

        let msg = PruneConsensusStatesMsg {
            heights_to_prune: vec![5],
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(client_state_pda, false),
                AccountMeta::new(pruner, true),
                AccountMeta::new(wrong_pubkey, false), // Wrong PDA
            ],
            data: crate::instruction::PruneConsensusStates { msg }.data(),
        };

        let accounts = vec![
            (client_state_pda, client_state_account),
            (pruner, create_submitter_account(10_000_000)),
            (wrong_pubkey, cs_account),
            (clock_pda, clock_account),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::test_helpers::PROGRAM_BINARY_PATH);
        let result = mollusk.process_instruction(&instruction, &accounts);

        assert_error_code(result, ErrorCode::InvalidAccount, "invalid_account");
    }

    #[test]
    fn test_prune_consensus_states_height_mismatch() {
        let pruner = Pubkey::new_unique();
        let chain_id = "test-chain";

        let client_state_pda = derive_client_state_pda(chain_id);
        let client_state_account = create_client_state_account_with_pruning_config(
            chain_id, 15, // latest_height
            10, // earliest_height
            1,  // consensus_state_count
        );

        let current_time = 100_000i64;
        let (clock_pda, clock_account) = create_clock_account(current_time);

        // Create consensus state with PDA for height 6, but store height 5 in data
        let old_timestamp =
            (current_time as u64).saturating_sub(CONSENSUS_STATE_PRUNING_GRACE_PERIOD + 1000);

        // First get the PDA for height 6 (what we'll provide to the instruction)
        let (cs_pda, _) = Pubkey::find_program_address(
            &[
                b"consensus_state",
                client_state_pda.as_ref(),
                &6u64.to_le_bytes(), // PDA for height 6
            ],
            &crate::ID,
        );

        // But create the account data with height 5 stored inside
        let (_, cs_account) =
            create_consensus_state_for_pruning(client_state_pda, 5, old_timestamp);

        let msg = PruneConsensusStatesMsg {
            heights_to_prune: vec![6], // Mismatch: PDA is for 6, but data has height 5
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(client_state_pda, false),
                AccountMeta::new(pruner, true),
                AccountMeta::new(cs_pda, false),
            ],
            data: crate::instruction::PruneConsensusStates { msg }.data(),
        };

        let accounts = vec![
            (client_state_pda, client_state_account),
            (pruner, create_submitter_account(10_000_000)),
            (cs_pda, cs_account),
            (clock_pda, clock_account),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::test_helpers::PROGRAM_BINARY_PATH);
        let result = mollusk.process_instruction(&instruction, &accounts);

        assert_error_code(result, ErrorCode::HeightMismatch, "height_mismatch");
    }

    #[test]
    fn test_prune_consensus_states_grace_period_not_met() {
        let pruner = Pubkey::new_unique();
        let chain_id = "test-chain";

        let client_state_pda = derive_client_state_pda(chain_id);
        let client_state_account = create_client_state_account_with_pruning_config(
            chain_id, 15, // latest_height
            10, // earliest_height
            1,  // consensus_state_count
        );

        let current_time = 100_000i64;
        let (clock_pda, clock_account) = create_clock_account(current_time);

        // Create consensus state with recent timestamp (grace period not met)
        let recent_timestamp = (current_time as u64).saturating_sub(1000); // Only 1000 seconds old
        let (cs_pda, cs_account) =
            create_consensus_state_for_pruning(client_state_pda, 5, recent_timestamp);

        let msg = PruneConsensusStatesMsg {
            heights_to_prune: vec![5],
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(client_state_pda, false),
                AccountMeta::new(pruner, true),
                AccountMeta::new(cs_pda, false),
            ],
            data: crate::instruction::PruneConsensusStates { msg }.data(),
        };

        let accounts = vec![
            (client_state_pda, client_state_account),
            (pruner, create_submitter_account(10_000_000)),
            (cs_pda, cs_account),
            (clock_pda, clock_account),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::test_helpers::PROGRAM_BINARY_PATH);
        let result = mollusk.process_instruction(&instruction, &accounts);

        assert_error_code(
            result,
            ErrorCode::PruningGracePeriodNotMet,
            "grace_period_not_met",
        );
    }

    #[test]
    fn test_prune_consensus_states_skip_already_pruned() {
        use anchor_lang::AnchorDeserialize;

        let pruner = Pubkey::new_unique();
        let chain_id = "test-chain";

        let client_state_pda = derive_client_state_pda(chain_id);
        let client_state_account = create_client_state_account_with_pruning_config(
            chain_id, 15, // latest_height
            10, // earliest_height
            3,  // consensus_state_count
        );

        let current_time = 100_000i64;
        let (clock_pda, clock_account) = create_clock_account(current_time);

        let old_timestamp =
            (current_time as u64).saturating_sub(CONSENSUS_STATE_PRUNING_GRACE_PERIOD + 1000);

        // Create one valid consensus state
        let (cs1_pda, cs1_account) =
            create_consensus_state_for_pruning(client_state_pda, 5, old_timestamp);

        // Create an already pruned consensus state (empty data)
        let (cs2_pda, _) = create_consensus_state_for_pruning(client_state_pda, 7, old_timestamp);
        let cs2_account_empty = solana_sdk::account::Account {
            lamports: 0,
            data: vec![], // Empty = already pruned
            owner: crate::ID,
            executable: false,
            rent_epoch: 0,
        };

        let msg = PruneConsensusStatesMsg {
            heights_to_prune: vec![5, 7], // 7 is already pruned
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(client_state_pda, false),
                AccountMeta::new(pruner, true),
                AccountMeta::new(cs1_pda, false),
                AccountMeta::new(cs2_pda, false),
            ],
            data: crate::instruction::PruneConsensusStates { msg }.data(),
        };

        let accounts = vec![
            (client_state_pda, client_state_account),
            (pruner, create_submitter_account(10_000_000)),
            (cs1_pda, cs1_account),
            (cs2_pda, cs2_account_empty),
            (clock_pda, clock_account),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::test_helpers::PROGRAM_BINARY_PATH);

        let checks = vec![
            Check::success(),
            // Only cs1 should be closed
            Check::account(&cs1_pda).lamports(0).build(),
            // cs2 was already closed
            Check::account(&cs2_pda).lamports(0).build(),
            // Pruner should only get rent from cs1
            Check::account(&pruner).lamports(11_500_000).build(), // 10M + 1.5M
        ];

        let result = mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);

        // Verify client_state.consensus_state_count was only decremented by 1
        let client_state_account = result
            .resulting_accounts
            .iter()
            .find(|(pubkey, _)| pubkey == &client_state_pda)
            .map(|(_, account)| account)
            .expect("Client state account not found");

        let mut data_slice = &client_state_account.data[8..]; // Skip 8-byte discriminator
        let client_state: crate::types::ClientState =
            crate::types::ClientState::deserialize(&mut data_slice).unwrap();

        assert_eq!(
            client_state.consensus_state_count, 2,
            "Should have decremented count by 1 (skipped already pruned)"
        );
    }
}
