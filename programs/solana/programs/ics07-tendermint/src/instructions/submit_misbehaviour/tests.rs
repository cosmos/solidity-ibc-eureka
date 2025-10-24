use crate::error::ErrorCode;
use crate::state::ConsensusStateStore;
use crate::test_helpers::PROGRAM_BINARY_PATH;
use crate::types::{ClientState, ConsensusState, IbcHeight, MisbehaviourMsg};
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
    trusted_consensus_state_1_pda: Pubkey,
    trusted_consensus_state_2_pda: Pubkey,
    accounts: Vec<(Pubkey, Account)>,
}

fn setup_test_accounts(
    chain_id: &str,
    height_1: u64,
    height_2: u64,
    client_frozen: bool,
    with_valid_consensus_states: bool,
) -> TestAccounts {
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

    // Create client state
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

    // Always add the consensus state accounts but they may be empty/uninitialized
    if with_valid_consensus_states {
        // Create consensus state 1
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
        // Add empty/uninitialized consensus state accounts
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

    TestAccounts {
        client_state_pda,
        trusted_consensus_state_1_pda,
        trusted_consensus_state_2_pda,
        accounts,
    }
}

fn create_misbehaviour_instruction(
    test_accounts: &TestAccounts,
    misbehaviour_bytes: Vec<u8>,
    client_id: &str,
) -> Instruction {
    let msg = MisbehaviourMsg {
        client_id: client_id.to_string(),
        misbehaviour: misbehaviour_bytes,
    };

    let instruction_data = crate::instruction::SubmitMisbehaviour { msg };

    Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(test_accounts.client_state_pda, false),
            AccountMeta::new_readonly(test_accounts.trusted_consensus_state_1_pda, false),
            AccountMeta::new_readonly(test_accounts.trusted_consensus_state_2_pda, false),
        ],
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
fn test_submit_misbehaviour_client_already_frozen() {
    let chain_id = "test-chain";
    let height_1 = 90;
    let height_2 = 95;

    let test_accounts = setup_test_accounts(chain_id, height_1, height_2, true, true);

    let misbehaviour_bytes = create_mock_misbehaviour_bytes(100, 100, true);

    let instruction = create_misbehaviour_instruction(&test_accounts, misbehaviour_bytes, chain_id);

    let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
    let checks = vec![Check::err(
        anchor_lang::error::Error::from(ErrorCode::ClientAlreadyFrozen).into(),
    )];
    mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
}

#[test]
fn test_submit_misbehaviour_without_consensus_states() {
    let chain_id = "test-chain";
    let height_1 = 90;
    let height_2 = 95;

    let test_accounts = setup_test_accounts(chain_id, height_1, height_2, false, false);

    let misbehaviour_bytes = create_mock_misbehaviour_bytes(100, 100, true);

    let instruction = create_misbehaviour_instruction(&test_accounts, misbehaviour_bytes, chain_id);

    let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
    let checks = vec![
        Check::err(anchor_lang::prelude::ProgramError::Custom(3012)), // AccountNotInitialized
    ];
    mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
}

#[test]
fn test_submit_misbehaviour_invalid_protobuf() {
    let chain_id = "test-chain";
    let height_1 = 90;
    let height_2 = 95;

    let test_accounts = setup_test_accounts(chain_id, height_1, height_2, false, true);

    // Create invalid misbehaviour bytes
    let invalid_bytes = vec![0xFF; 100];

    let instruction = create_misbehaviour_instruction(&test_accounts, invalid_bytes, chain_id);

    let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
    let checks = vec![
        Check::err(anchor_lang::prelude::ProgramError::Custom(0x1778)), // InvalidHeader
    ];
    mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
}

#[test]
fn test_submit_misbehaviour_empty_misbehaviour_bytes() {
    let chain_id = "test-chain";
    let height_1 = 90;
    let height_2 = 95;

    let test_accounts = setup_test_accounts(chain_id, height_1, height_2, false, true);

    let instruction = create_misbehaviour_instruction(&test_accounts, vec![], chain_id);

    let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
    let checks = vec![
        Check::err(anchor_lang::prelude::ProgramError::Custom(0x1778)), // InvalidHeader
    ];
    mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
}

#[test]
fn test_submit_misbehaviour_with_mismatched_heights() {
    let chain_id = "test-chain";
    let height_1 = 90;
    let height_2 = 95;

    // Create test accounts with different heights than what the misbehaviour expects
    let test_accounts = setup_test_accounts(chain_id, height_1 + 10, height_2 + 10, false, true);

    let misbehaviour_bytes = create_mock_misbehaviour_bytes(100, 100, true);

    let instruction = create_misbehaviour_instruction(&test_accounts, misbehaviour_bytes, chain_id);

    let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
    let checks = vec![
        Check::err(anchor_lang::prelude::ProgramError::Custom(0x1778)), // InvalidHeader
    ];
    mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
}
