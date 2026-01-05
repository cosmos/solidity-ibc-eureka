use crate::error::ErrorCode;
use crate::state::{ConsensusStateStore, MisbehaviourChunk};
use crate::test_helpers::PROGRAM_BINARY_PATH;
use crate::types::{AppState, ClientState, ConsensusState, IbcHeight};
use anchor_lang::solana_program::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    sysvar::clock::Clock,
};
use anchor_lang::AccountSerialize;
use anchor_lang::InstructionData;
use mollusk_svm::result::Check;
use mollusk_svm::Mollusk;
use solana_sdk::account::Account;

struct TestAccounts {
    client_state_pda: Pubkey,
    app_state_pda: Pubkey,
    trusted_consensus_state_1_pda: Pubkey,
    trusted_consensus_state_2_pda: Pubkey,
    submitter: Pubkey,
    chunk_pdas: Vec<Pubkey>,
    accounts: Vec<(Pubkey, Account)>,
}

struct TestSetupConfig<'a> {
    chain_id: &'a str,
    height_1: u64,
    height_2: u64,
    submitter: Pubkey,
    client_frozen: bool,
    with_valid_consensus_states: bool,
    with_chunks: bool,
    misbehaviour_bytes: &'a [u8],
}

fn setup_test_accounts(config: TestSetupConfig) -> TestAccounts {
    let TestSetupConfig {
        chain_id,
        height_1,
        height_2,
        submitter,
        client_frozen,
        with_valid_consensus_states,
        with_chunks,
        misbehaviour_bytes,
    } = config;
    let client_state_pda = Pubkey::find_program_address(
        &[crate::types::ClientState::SEED, chain_id.as_bytes()],
        &crate::ID,
    )
    .0;

    let trusted_consensus_state_1_pda = Pubkey::find_program_address(
        &[
            crate::state::ConsensusStateStore::SEED,
            client_state_pda.as_ref(),
            &height_1.to_le_bytes(),
        ],
        &crate::ID,
    )
    .0;

    let trusted_consensus_state_2_pda = Pubkey::find_program_address(
        &[
            crate::state::ConsensusStateStore::SEED,
            client_state_pda.as_ref(),
            &height_2.to_le_bytes(),
        ],
        &crate::ID,
    )
    .0;

    let (app_state_pda, _) = Pubkey::find_program_address(&[AppState::SEED], &crate::ID);

    let client_state = ClientState {
        chain_id: chain_id.to_string(),
        trust_level_numerator: 2,
        trust_level_denominator: 3,
        trusting_period: 86400,
        unbonding_period: 172_800,
        max_clock_drift: 600,
        frozen_height: if client_frozen {
            IbcHeight {
                revision_number: 0,
                revision_height: 999,
            }
        } else {
            IbcHeight {
                revision_number: 0,
                revision_height: 0,
            }
        },
        latest_height: IbcHeight {
            revision_number: 0,
            revision_height: 200,
        },
    };

    let mut client_data = vec![];
    client_state.try_serialize(&mut client_data).unwrap();

    let mut accounts = vec![(
        client_state_pda,
        Account {
            lamports: 1_000_000,
            data: client_data,
            owner: crate::ID,
            executable: false,
            rent_epoch: 0,
        },
    )];

    accounts.push((
        submitter,
        Account {
            lamports: 10_000_000_000,
            data: vec![],
            owner: solana_sdk::system_program::ID,
            executable: false,
            rent_epoch: 0,
        },
    ));

    // Add app_state account
    let app_state = AppState {
        access_manager: access_manager::ID,
        _reserved: [0; 256],
    };
    let mut app_state_data = vec![];
    app_state.try_serialize(&mut app_state_data).unwrap();
    accounts.push((
        app_state_pda,
        Account {
            lamports: 1_000_000,
            data: app_state_data,
            owner: crate::ID,
            executable: false,
            rent_epoch: 0,
        },
    ));

    // Add access manager account
    let (_, access_manager_account) =
        crate::test_helpers::access_control::create_access_manager_account(
            submitter,
            vec![submitter],
        );
    let (access_manager_pda, _) =
        solana_ibc_types::access_manager::AccessManager::pda(access_manager::ID);
    accounts.push((access_manager_pda, access_manager_account));

    if with_valid_consensus_states {
        let consensus_state_1 = ConsensusState {
            timestamp: 1_000_000_000_000_000_000, // nanoseconds
            root: [1u8; 32],
            next_validators_hash: [2u8; 32],
        };

        let consensus_state_store_1 = ConsensusStateStore {
            height: height_1,
            consensus_state: consensus_state_1,
        };

        let mut consensus_data_1 = vec![];
        consensus_state_store_1
            .try_serialize(&mut consensus_data_1)
            .unwrap();

        accounts.push((
            trusted_consensus_state_1_pda,
            Account {
                lamports: 1_000_000,
                data: consensus_data_1,
                owner: crate::ID,
                executable: false,
                rent_epoch: 0,
            },
        ));

        let consensus_state_2 = ConsensusState {
            timestamp: 2_000_000_000_000_000_000, // nanoseconds
            root: [3u8; 32],
            next_validators_hash: [4u8; 32],
        };

        let consensus_state_store_2 = ConsensusStateStore {
            height: height_2,
            consensus_state: consensus_state_2,
        };

        let mut consensus_data_2 = vec![];
        consensus_state_store_2
            .try_serialize(&mut consensus_data_2)
            .unwrap();

        accounts.push((
            trusted_consensus_state_2_pda,
            Account {
                lamports: 1_000_000,
                data: consensus_data_2,
                owner: crate::ID,
                executable: false,
                rent_epoch: 0,
            },
        ));
    } else {
        accounts.push((
            trusted_consensus_state_1_pda,
            Account {
                lamports: 0,
                data: vec![],
                owner: solana_sdk::system_program::ID,
                executable: false,
                rent_epoch: 0,
            },
        ));

        accounts.push((
            trusted_consensus_state_2_pda,
            Account {
                lamports: 0,
                data: vec![],
                owner: solana_sdk::system_program::ID,
                executable: false,
                rent_epoch: 0,
            },
        ));
    }

    let clock = Clock {
        slot: 0,
        epoch_start_timestamp: 0,
        epoch: 0,
        leader_schedule_epoch: 0,
        unix_timestamp: 1_700_000_000, // Current time
    };
    let clock_data = bincode::serialize(&clock).unwrap();

    accounts.push((
        solana_sdk::sysvar::clock::ID,
        Account {
            lamports: 1,
            data: clock_data,
            owner: solana_sdk::native_loader::ID,
            executable: false,
            rent_epoch: 0,
        },
    ));

    // Add instructions sysvar for CPI validation
    accounts.push((
        anchor_lang::solana_program::sysvar::instructions::ID,
        crate::test_helpers::create_instructions_sysvar_account(),
    ));

    let mut chunk_pdas = vec![];
    if with_chunks {
        const CHUNK_SIZE: usize = 700;
        let num_chunks = misbehaviour_bytes.len().div_ceil(CHUNK_SIZE);

        for i in 0..num_chunks {
            let chunk_pda = Pubkey::find_program_address(
                &[
                    crate::state::MisbehaviourChunk::SEED,
                    submitter.as_ref(),
                    chain_id.as_bytes(),
                    &[i as u8],
                ],
                &crate::ID,
            )
            .0;

            chunk_pdas.push(chunk_pda);

            let start = i * CHUNK_SIZE;
            let end = std::cmp::min(start + CHUNK_SIZE, misbehaviour_bytes.len());
            let chunk_data_slice = &misbehaviour_bytes[start..end];

            let chunk = MisbehaviourChunk {
                chunk_data: chunk_data_slice.to_vec(),
            };

            let mut chunk_data = vec![];
            chunk.try_serialize(&mut chunk_data).unwrap();

            accounts.push((
                chunk_pda,
                Account {
                    lamports: 1_500_000,
                    data: chunk_data,
                    owner: crate::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ));
        }
    }

    TestAccounts {
        client_state_pda,
        app_state_pda,
        trusted_consensus_state_1_pda,
        trusted_consensus_state_2_pda,
        submitter,
        chunk_pdas,
        accounts,
    }
}

fn create_assemble_instruction(test_accounts: &TestAccounts, client_id: &str) -> Instruction {
    let chunk_count = test_accounts.chunk_pdas.len() as u8;
    let instruction_data = crate::instruction::AssembleAndSubmitMisbehaviour {
        client_id: client_id.to_string(),
        chunk_count,
    };

    let (access_manager_pda, _) =
        solana_ibc_types::access_manager::AccessManager::pda(access_manager::ID);

    let mut account_metas = vec![
        AccountMeta::new(test_accounts.client_state_pda, false),
        AccountMeta::new_readonly(test_accounts.app_state_pda, false),
        AccountMeta::new_readonly(access_manager_pda, false),
        AccountMeta::new_readonly(test_accounts.trusted_consensus_state_1_pda, false),
        AccountMeta::new_readonly(test_accounts.trusted_consensus_state_2_pda, false),
        AccountMeta::new(test_accounts.submitter, true),
        AccountMeta::new_readonly(anchor_lang::solana_program::sysvar::instructions::ID, false),
    ];

    for chunk_pda in &test_accounts.chunk_pdas {
        account_metas.push(AccountMeta::new(*chunk_pda, false));
    }

    Instruction {
        program_id: crate::ID,
        accounts: account_metas,
        data: instruction_data.data(),
    }
}

fn create_mock_misbehaviour_bytes(
    header1_height: u64,
    header2_height: u64,
    conflicting: bool,
) -> Vec<u8> {
    crate::test_helpers::fixtures::misbehaviour::create_mock_tendermint_misbehaviour(
        "test-chain",
        header1_height,
        header2_height,
        90,
        95,
        conflicting,
    )
}

#[test]
fn test_assemble_and_submit_misbehaviour_client_already_frozen() {
    let chain_id = "test-chain";
    let height_1 = 90;
    let height_2 = 95;
    let submitter = Pubkey::new_unique();

    let misbehaviour_bytes = create_mock_misbehaviour_bytes(100, 100, true);

    let test_accounts = setup_test_accounts(TestSetupConfig {
        chain_id,
        height_1,
        height_2,
        submitter,
        client_frozen: true,
        with_valid_consensus_states: true,
        with_chunks: true,
        misbehaviour_bytes: &misbehaviour_bytes,
    });

    let instruction = create_assemble_instruction(&test_accounts, chain_id);

    let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
    let checks = vec![Check::err(
        anchor_lang::error::Error::from(ErrorCode::ClientAlreadyFrozen).into(),
    )];
    mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
}

#[test]
fn test_assemble_and_submit_misbehaviour_wrong_chunk_pda() {
    let chain_id = "test-chain";
    let height_1 = 90;
    let height_2 = 95;
    let submitter = Pubkey::new_unique();

    let misbehaviour_bytes = create_mock_misbehaviour_bytes(100, 100, true);

    let mut test_accounts = setup_test_accounts(TestSetupConfig {
        chain_id,
        height_1,
        height_2,
        submitter,
        client_frozen: false,
        with_valid_consensus_states: true,
        with_chunks: true,
        misbehaviour_bytes: &misbehaviour_bytes,
    });

    // Replace one chunk PDA with a wrong one - both in the PDAs list and accounts
    if !test_accounts.chunk_pdas.is_empty() {
        let old_chunk_pda = test_accounts.chunk_pdas[0];
        let wrong_chunk_pda = Pubkey::new_unique();

        // Update the PDA list
        test_accounts.chunk_pdas[0] = wrong_chunk_pda;

        // Find and update the account in the accounts list
        if let Some(account_entry) = test_accounts
            .accounts
            .iter_mut()
            .find(|(k, _)| *k == old_chunk_pda)
        {
            account_entry.0 = wrong_chunk_pda;
        }
    }

    let instruction = create_assemble_instruction(&test_accounts, chain_id);

    let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
    let checks = vec![Check::err(
        anchor_lang::error::Error::from(ErrorCode::InvalidChunkAccount).into(),
    )];
    mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
}
