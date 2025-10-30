use super::*;
use crate::error::ErrorCode;
use crate::state::ConsensusStateStore;
use crate::test_helpers::fixtures::assert_error_code;
use crate::test_helpers::PROGRAM_BINARY_PATH;
use crate::types::{ClientState, ConsensusState, IbcHeight};
use anchor_lang::solana_program::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    system_program,
};
use anchor_lang::{AccountSerialize, InstructionData};
use mollusk_svm::Mollusk;
use solana_sdk::account::Account;

struct TestAccounts {
    rent_receiver: Pubkey,
    client_state_pda: Pubkey,
    consensus_state_pdas: Vec<Pubkey>,
    payers: Vec<Pubkey>,
    accounts: Vec<(Pubkey, Account)>,
}

fn setup_test_accounts(
    chain_id: &str,
    heights_to_prune: Vec<u64>,
    consensus_state_heights: Vec<u64>,
    create_consensus_accounts: bool,
) -> TestAccounts {
    let rent_receiver = Pubkey::new_unique();

    let client_state_pda = Pubkey::find_program_address(
        &[crate::types::ClientState::SEED, chain_id.as_bytes()],
        &crate::ID,
    )
    .0;

    let mut consensus_state_pdas = vec![];
    for height in &heights_to_prune {
        let pda = Pubkey::find_program_address(
            &[
                ConsensusStateStore::SEED,
                client_state_pda.as_ref(),
                &height.to_le_bytes(),
            ],
            &crate::ID,
        )
        .0;
        consensus_state_pdas.push(pda);
    }

    let mut accounts = vec![];

    // Add rent receiver account
    accounts.push((
        rent_receiver,
        Account {
            lamports: 1_000_000_000,
            data: vec![],
            owner: system_program::ID,
            executable: false,
            rent_epoch: 0,
        },
    ));

    // Add client state account
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
            revision_number: 1,
            revision_height: 1000,
        },
        consensus_state_heights,
        consensus_state_heights_to_prune: heights_to_prune.clone(),
    };

    let mut client_data = vec![];
    client_state.try_serialize(&mut client_data).unwrap();

    accounts.push((
        client_state_pda,
        Account {
            lamports: 5_000_000,
            data: client_data,
            owner: crate::ID,
            executable: false,
            rent_epoch: 0,
        },
    ));

    // Add consensus state accounts
    let mut payers = vec![];
    if create_consensus_accounts {
        for (i, height) in heights_to_prune.iter().enumerate() {
            let payer = Pubkey::new_unique();
            payers.push(payer);

            let consensus_store = ConsensusStateStore {
                height: *height,
                consensus_state: ConsensusState {
                    timestamp: 1_000_000_000 + (*height * 1000),
                    root: [i as u8; 32],
                    next_validators_hash: [(i + 1) as u8; 32],
                },
                payer,
            };

            let mut data = vec![];
            consensus_store.try_serialize(&mut data).unwrap();

            accounts.push((
                consensus_state_pdas[i],
                Account {
                    lamports: 2_000_000, // Rent to be reclaimed
                    data,
                    owner: crate::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ));

            // Add payer account (they need to exist to receive rent)
            accounts.push((
                payer,
                Account {
                    lamports: 1_000_000,
                    data: vec![],
                    owner: system_program::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ));
        }
    }

    TestAccounts {
        rent_receiver,
        client_state_pda,
        consensus_state_pdas,
        payers,
        accounts,
    }
}

fn create_prune_instruction(
    test_accounts: &TestAccounts,
    chain_id: String,
    consensus_accounts_to_include: &[usize],
) -> Instruction {
    let instruction_data = crate::instruction::PruneConsensusStates { chain_id };

    let mut account_metas = vec![
        AccountMeta::new(test_accounts.client_state_pda, false),
        AccountMeta::new(test_accounts.rent_receiver, true),
    ];

    // Add selected consensus state accounts and their payers as remaining accounts
    for &index in consensus_accounts_to_include {
        // Add consensus state account
        account_metas.push(AccountMeta::new(
            test_accounts.consensus_state_pdas[index],
            false,
        ));
        // Add payer account (required for rent refund)
        account_metas.push(AccountMeta::new(test_accounts.payers[index], false));
    }

    Instruction {
        program_id: crate::ID,
        accounts: account_metas,
        data: instruction_data.data(),
    }
}

fn assert_instruction_succeeds(
    instruction: &Instruction,
    accounts: &[(Pubkey, Account)],
) -> mollusk_svm::result::InstructionResult {
    let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
    let result = mollusk.process_instruction(instruction, accounts);

    if !matches!(
        result.program_result,
        mollusk_svm::result::ProgramResult::Success
    ) {
        panic!("Instruction failed: {:?}", result.program_result);
    }

    result
}

#[test]
fn test_prune_consensus_states_happy_path() {
    let chain_id = "test-chain-1";
    let heights_to_prune = vec![100, 200, 300];
    let consensus_state_heights = vec![400, 500, 600]; // Active heights

    let test_accounts = setup_test_accounts(
        chain_id,
        heights_to_prune,
        consensus_state_heights,
        true, // Create consensus accounts
    );

    let instruction = create_prune_instruction(&test_accounts, chain_id.to_string(), &[0, 1, 2]);

    let initial_receiver_lamports = test_accounts
        .accounts
        .iter()
        .find(|(k, _)| *k == test_accounts.rent_receiver)
        .unwrap()
        .1
        .lamports;

    let result = assert_instruction_succeeds(&instruction, &test_accounts.accounts);

    // Verify rent receiver got 5% bounty
    let final_receiver_lamports = result
        .resulting_accounts
        .iter()
        .find(|(k, _)| *k == test_accounts.rent_receiver)
        .unwrap()
        .1
        .lamports;

    // Total rent from 3 consensus states: 2M * 3 = 6M
    // Pruner bounty: 5% of 6M = 300k
    let total_rent = 2_000_000 * 3;
    let expected_bounty = total_rent * 5 / 100;
    assert_eq!(
        final_receiver_lamports,
        initial_receiver_lamports + expected_bounty
    );

    // Verify payers received 95% of their rent
    for (i, payer) in test_accounts.payers.iter().enumerate() {
        let payer_lamports = result
            .resulting_accounts
            .iter()
            .find(|(k, _)| k == payer)
            .unwrap()
            .1
            .lamports;
        // Each payer should get 95% of 2M = 1.9M, plus their initial 1M = 2.9M total
        let payer_refund = 2_000_000 * 95 / 100;
        assert_eq!(
            payer_lamports,
            1_000_000 + payer_refund,
            "Payer {i} did not receive correct refund",
        );
    }

    // Verify consensus state accounts were closed
    for pda in &test_accounts.consensus_state_pdas {
        let account = &result
            .resulting_accounts
            .iter()
            .find(|(k, _)| k == pda)
            .unwrap()
            .1;
        assert_eq!(account.lamports, 0, "Consensus state should be closed");
    }

    // Verify to_prune list was cleared
    let client_state_account = &result
        .resulting_accounts
        .iter()
        .find(|(k, _)| *k == test_accounts.client_state_pda)
        .unwrap()
        .1;

    let client_state: ClientState =
        ClientState::try_deserialize(&mut &client_state_account.data[..]).unwrap();

    assert_eq!(
        client_state.consensus_state_heights_to_prune.len(),
        0,
        "to_prune list should be empty"
    );
}

#[test]
fn test_prune_partial_heights() {
    let chain_id = "test-chain-2";
    let heights_to_prune = vec![100, 200, 300];
    let consensus_state_heights = vec![400, 500];

    let test_accounts =
        setup_test_accounts(chain_id, heights_to_prune, consensus_state_heights, true);

    // Only prune first two heights
    let instruction = create_prune_instruction(&test_accounts, chain_id.to_string(), &[0, 1]);

    let result = assert_instruction_succeeds(&instruction, &test_accounts.accounts);

    // Verify first two accounts were closed
    for i in 0..2 {
        let account = &result
            .resulting_accounts
            .iter()
            .find(|(k, _)| *k == test_accounts.consensus_state_pdas[i])
            .unwrap()
            .1;
        assert_eq!(account.lamports, 0, "Account {i} should be closed");
    }

    // Verify third account still has lamports
    let third_account = &result
        .resulting_accounts
        .iter()
        .find(|(k, _)| *k == test_accounts.consensus_state_pdas[2])
        .unwrap()
        .1;
    assert_eq!(
        third_account.lamports, 2_000_000,
        "Third account should still be open"
    );

    // Verify to_prune list only has height 300 remaining
    let client_state_account = &result
        .resulting_accounts
        .iter()
        .find(|(k, _)| *k == test_accounts.client_state_pda)
        .unwrap()
        .1;

    let client_state: ClientState =
        ClientState::try_deserialize(&mut &client_state_account.data[..]).unwrap();

    assert_eq!(client_state.consensus_state_heights_to_prune.len(), 1);
    assert_eq!(client_state.consensus_state_heights_to_prune[0], 300);
}

#[test]
fn test_prune_skips_heights_not_in_to_prune_list() {
    let chain_id = "test-chain-3";
    let heights_to_prune = vec![100]; // Only height 100 is marked for pruning
    let consensus_state_heights = vec![200, 300];

    // Create consensus state for height 100 (in to_prune)
    let mut test_accounts =
        setup_test_accounts(chain_id, heights_to_prune, consensus_state_heights, true);

    // Manually add a consensus state for height 200 (NOT in to_prune)
    let height_200_pda = Pubkey::find_program_address(
        &[
            ConsensusStateStore::SEED,
            test_accounts.client_state_pda.as_ref(),
            &200u64.to_le_bytes(),
        ],
        &crate::ID,
    )
    .0;

    let payer_200 = Pubkey::new_unique();
    let consensus_store_200 = ConsensusStateStore {
        height: 200,
        consensus_state: ConsensusState {
            timestamp: 1_000_200_000,
            root: [99u8; 32],
            next_validators_hash: [98u8; 32],
        },
        payer: payer_200,
    };

    let mut data_200 = vec![];
    consensus_store_200.try_serialize(&mut data_200).unwrap();

    test_accounts.accounts.push((
        height_200_pda,
        Account {
            lamports: 2_000_000,
            data: data_200,
            owner: crate::ID,
            executable: false,
            rent_epoch: 0,
        },
    ));

    // Add payer account for height 200
    test_accounts.accounts.push((
        payer_200,
        Account {
            lamports: 1_000_000,
            data: vec![],
            owner: system_program::ID,
            executable: false,
            rent_epoch: 0,
        },
    ));

    // Try to prune both heights (height 200 should be skipped because not in to_prune list)
    let instruction_data = crate::instruction::PruneConsensusStates {
        chain_id: chain_id.to_string(),
    };

    let account_metas = vec![
        AccountMeta::new(test_accounts.client_state_pda, false),
        AccountMeta::new(test_accounts.rent_receiver, true),
        AccountMeta::new(test_accounts.consensus_state_pdas[0], false), // Height 100 - in to_prune
        AccountMeta::new(test_accounts.payers[0], false),               // Payer for height 100
        AccountMeta::new(height_200_pda, false), // Height 200 - NOT in to_prune
        AccountMeta::new(payer_200, false), // Payer for height 200 (won't be used since 200 is skipped)
    ];

    let instruction = Instruction {
        program_id: crate::ID,
        accounts: account_metas,
        data: instruction_data.data(),
    };

    let result = assert_instruction_succeeds(&instruction, &test_accounts.accounts);

    // Verify height 100 was closed
    let account_100 = &result
        .resulting_accounts
        .iter()
        .find(|(k, _)| *k == test_accounts.consensus_state_pdas[0])
        .unwrap()
        .1;
    assert_eq!(account_100.lamports, 0, "Height 100 should be closed");

    // Verify height 200 was NOT closed (skipped because not in to_prune)
    let account_200 = &result
        .resulting_accounts
        .iter()
        .find(|(k, _)| *k == height_200_pda)
        .unwrap()
        .1;
    assert_eq!(
        account_200.lamports, 2_000_000,
        "Height 200 should NOT be closed"
    );

    // Verify to_prune list is now empty (only height 100 was removed)
    let client_state_account = &result
        .resulting_accounts
        .iter()
        .find(|(k, _)| *k == test_accounts.client_state_pda)
        .unwrap()
        .1;

    let client_state: ClientState =
        ClientState::try_deserialize(&mut &client_state_account.data[..]).unwrap();

    assert_eq!(
        client_state.consensus_state_heights_to_prune.len(),
        0,
        "to_prune list should be empty"
    );
}

#[test]
fn test_prune_with_empty_to_prune_list() {
    let chain_id = "test-chain-4";
    let heights_to_prune = vec![]; // Empty to_prune list
    let consensus_state_heights = vec![100, 200];

    let test_accounts = setup_test_accounts(
        chain_id,
        heights_to_prune,
        consensus_state_heights,
        false, // Don't create consensus accounts
    );

    let instruction = create_prune_instruction(&test_accounts, chain_id.to_string(), &[]);

    let result = assert_instruction_succeeds(&instruction, &test_accounts.accounts);

    // Verify client state is unchanged
    let client_state_account = &result
        .resulting_accounts
        .iter()
        .find(|(k, _)| *k == test_accounts.client_state_pda)
        .unwrap()
        .1;

    let client_state: ClientState =
        ClientState::try_deserialize(&mut &client_state_account.data[..]).unwrap();

    assert_eq!(
        client_state.consensus_state_heights_to_prune.len(),
        0,
        "to_prune list should remain empty"
    );
}

#[test]
fn test_prune_skips_empty_accounts() {
    let chain_id = "test-chain-5";
    let heights_to_prune = vec![100, 200];
    let consensus_state_heights = vec![300];

    // Create test accounts but only populate first consensus state
    let mut test_accounts = setup_test_accounts(
        chain_id,
        heights_to_prune,
        consensus_state_heights,
        false, // Don't auto-create
    );

    // Manually add only first consensus state (populated)
    let payer = Pubkey::new_unique();
    let consensus_store = ConsensusStateStore {
        height: 100,
        consensus_state: ConsensusState {
            timestamp: 1_000_100_000,
            root: [1u8; 32],
            next_validators_hash: [2u8; 32],
        },
        payer,
    };

    let mut data = vec![];
    consensus_store.try_serialize(&mut data).unwrap();

    test_accounts.accounts.push((
        test_accounts.consensus_state_pdas[0],
        Account {
            lamports: 2_000_000,
            data,
            owner: crate::ID,
            executable: false,
            rent_epoch: 0,
        },
    ));

    // Add payer for first consensus state
    test_accounts.payers.push(payer);
    test_accounts.accounts.push((
        payer,
        Account {
            lamports: 1_000_000,
            data: vec![],
            owner: system_program::ID,
            executable: false,
            rent_epoch: 0,
        },
    ));

    // Add placeholder payer for second (empty) consensus state
    // This is needed because create_prune_instruction expects payers[1] to exist
    let placeholder_payer = Pubkey::new_unique();
    test_accounts.payers.push(placeholder_payer);
    test_accounts.accounts.push((
        placeholder_payer,
        Account {
            lamports: 1_000_000,
            data: vec![],
            owner: system_program::ID,
            executable: false,
            rent_epoch: 0,
        },
    ));

    // Add second consensus state as empty
    test_accounts.accounts.push((
        test_accounts.consensus_state_pdas[1],
        Account {
            lamports: 0,
            data: vec![],
            owner: system_program::ID,
            executable: false,
            rent_epoch: 0,
        },
    ));

    let instruction = create_prune_instruction(&test_accounts, chain_id.to_string(), &[0, 1]);

    let result = assert_instruction_succeeds(&instruction, &test_accounts.accounts);

    // Verify first account was closed
    let account_100 = &result
        .resulting_accounts
        .iter()
        .find(|(k, _)| *k == test_accounts.consensus_state_pdas[0])
        .unwrap()
        .1;
    assert_eq!(account_100.lamports, 0, "First account should be closed");

    // Verify to_prune list only has height 200 remaining (empty account was skipped)
    let client_state_account = &result
        .resulting_accounts
        .iter()
        .find(|(k, _)| *k == test_accounts.client_state_pda)
        .unwrap()
        .1;

    let client_state: ClientState =
        ClientState::try_deserialize(&mut &client_state_account.data[..]).unwrap();

    assert_eq!(
        client_state.consensus_state_heights_to_prune.len(),
        1,
        "Height 200 should remain in to_prune (empty account skipped)"
    );
    assert_eq!(client_state.consensus_state_heights_to_prune[0], 200);
}

#[test]
fn test_prune_verifies_pda() {
    let chain_id = "test-chain-6";
    let heights_to_prune = vec![100];
    let consensus_state_heights = vec![200];

    let test_accounts =
        setup_test_accounts(chain_id, heights_to_prune, consensus_state_heights, true);

    // Create instruction with WRONG PDA (random pubkey)
    let wrong_pda = Pubkey::new_unique();

    let instruction_data = crate::instruction::PruneConsensusStates {
        chain_id: chain_id.to_string(),
    };

    let account_metas = vec![
        AccountMeta::new(test_accounts.client_state_pda, false),
        AccountMeta::new(test_accounts.rent_receiver, true),
        AccountMeta::new(wrong_pda, false), // Wrong PDA!
    ];

    let instruction = Instruction {
        program_id: crate::ID,
        accounts: account_metas,
        data: instruction_data.data(),
    };

    // Add the wrong account to our test accounts
    let mut modified_accounts = test_accounts.accounts;
    let consensus_store = ConsensusStateStore {
        height: 100,
        consensus_state: ConsensusState {
            timestamp: 1_000_100_000,
            root: [1u8; 32],
            next_validators_hash: [2u8; 32],
        },
        payer: Pubkey::new_unique(),
    };

    let mut data = vec![];
    consensus_store.try_serialize(&mut data).unwrap();

    modified_accounts.push((
        wrong_pda,
        Account {
            lamports: 2_000_000,
            data,
            owner: crate::ID,
            executable: false,
            rent_epoch: 0,
        },
    ));

    let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
    let result = mollusk.process_instruction(&instruction, &modified_accounts);

    assert_error_code(
        result,
        ErrorCode::AccountValidationFailed,
        "test_prune_verifies_pda",
    );
}

#[test]
fn test_prune_fails_without_payer_account() {
    let chain_id = "test-chain";
    let heights_to_prune = vec![100];
    let consensus_state_heights = vec![100, 200, 300];

    let test_accounts =
        setup_test_accounts(chain_id, heights_to_prune, consensus_state_heights, true);

    let instruction_data = crate::instruction::PruneConsensusStates {
        chain_id: chain_id.to_string(),
    };

    // Build instruction WITHOUT payer account in remaining accounts
    let account_metas = vec![
        AccountMeta::new(test_accounts.client_state_pda, false),
        AccountMeta::new(test_accounts.rent_receiver, true),
        AccountMeta::new(test_accounts.consensus_state_pdas[0], false), // Only consensus state, no payer
    ];

    let instruction = Instruction {
        program_id: crate::ID,
        accounts: account_metas,
        data: instruction_data.data(),
    };

    let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
    let result = mollusk.process_instruction(&instruction, &test_accounts.accounts);

    // Should fail with MissingAccount error
    assert_error_code(
        result,
        ErrorCode::MissingAccount,
        "test_prune_fails_without_payer_account",
    );
}
