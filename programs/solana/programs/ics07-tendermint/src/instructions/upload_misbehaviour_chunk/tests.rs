use crate::error::ErrorCode;
use crate::state::{MisbehaviourChunk, CHUNK_DATA_SIZE};
use crate::test_helpers::PROGRAM_BINARY_PATH;
use crate::types::{ClientState, IbcHeight, UploadMisbehaviourChunkParams};
use anchor_lang::solana_program::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    system_program,
};
use anchor_lang::{AccountDeserialize, AccountSerialize, InstructionData};
use mollusk_svm::{program::keyed_account_for_system_program, result::Check, Mollusk};
use solana_sdk::account::Account;

struct TestAccounts {
    submitter: Pubkey,
    chunk_pda: Pubkey,
    client_state_pda: Pubkey,
    accounts: Vec<(Pubkey, Account)>,
}

fn setup_test_accounts(
    client_id: &str,
    chunk_index: u8,
    submitter: Pubkey,
    with_existing_client: bool,
) -> TestAccounts {
    let chunk_pda = Pubkey::find_program_address(
        &[
            crate::state::MisbehaviourChunk::SEED,
            submitter.as_ref(),
            client_id.as_bytes(),
            &[chunk_index],
        ],
        &crate::ID,
    )
    .0;

    let client_state_pda = Pubkey::find_program_address(
        &[crate::types::ClientState::SEED, client_id.as_bytes()],
        &crate::ID,
    )
    .0;

    let mut accounts = vec![
        (
            chunk_pda,
            Account {
                lamports: 0,
                data: vec![],
                owner: system_program::ID,
                executable: false,
                rent_epoch: 0,
            },
        ),
        (
            submitter,
            Account {
                lamports: 10_000_000_000,
                data: vec![],
                owner: system_program::ID,
                executable: false,
                rent_epoch: 0,
            },
        ),
        keyed_account_for_system_program(),
    ];

    if with_existing_client {
        let client_state = ClientState {
            chain_id: client_id.to_string(),
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
                revision_height: 100,
            },
        };

        let mut client_data = vec![];
        client_state
            .try_serialize(&mut client_data)
            .expect("Failed to serialize client state");

        accounts.push((
            client_state_pda,
            Account {
                lamports: 1_000_000,
                data: client_data,
                owner: crate::ID,
                executable: false,
                rent_epoch: 0,
            },
        ));
    }

    TestAccounts {
        submitter,
        chunk_pda,
        client_state_pda,
        accounts,
    }
}

fn create_upload_instruction(
    test_accounts: &TestAccounts,
    params: UploadMisbehaviourChunkParams,
) -> Instruction {
    let instruction_data = crate::instruction::UploadMisbehaviourChunk { params };

    Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(test_accounts.chunk_pda, false),
            AccountMeta::new_readonly(test_accounts.client_state_pda, false),
            AccountMeta::new(test_accounts.submitter, true),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
        data: instruction_data.data(),
    }
}

#[test]
fn test_upload_misbehaviour_chunk_success() {
    let client_id = "test-client";
    let chunk_index = 0;
    let submitter = Pubkey::new_unique();

    let test_accounts = setup_test_accounts(client_id, chunk_index, submitter, true);

    let params = UploadMisbehaviourChunkParams {
        client_id: client_id.to_string(),
        chunk_index,
        chunk_data: vec![1u8; 100],
    };

    let instruction = create_upload_instruction(&test_accounts, params);

    let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
    let result = mollusk.process_instruction(&instruction, &test_accounts.accounts);

    assert!(
        matches!(
            result.program_result,
            mollusk_svm::result::ProgramResult::Success
        ),
        "Instruction should succeed"
    );

    let chunk_account = result
        .get_account(&test_accounts.chunk_pda)
        .expect("chunk account should exist");
    let chunk = MisbehaviourChunk::try_deserialize(&mut chunk_account.data.as_ref())
        .expect("should deserialize chunk");
    assert_eq!(chunk.chunk_data.len(), 100);
}

#[test]
fn test_upload_chunk_with_frozen_client_fails() {
    let client_id = "test-client";
    let chunk_index = 0;
    let submitter = Pubkey::new_unique();

    let mut test_accounts = setup_test_accounts(client_id, chunk_index, submitter, true);

    let client_state_pda = Pubkey::find_program_address(
        &[crate::types::ClientState::SEED, client_id.as_bytes()],
        &crate::ID,
    )
    .0;

    if let Some((_, account)) = test_accounts
        .accounts
        .iter_mut()
        .find(|(key, _)| *key == client_state_pda)
    {
        let frozen_client_state = ClientState {
            chain_id: client_id.to_string(),
            trust_level_numerator: 2,
            trust_level_denominator: 3,
            trusting_period: 86400,
            unbonding_period: 172_800,
            max_clock_drift: 600,
            frozen_height: IbcHeight {
                revision_number: 0,
                revision_height: 100,
            },
            latest_height: IbcHeight {
                revision_number: 0,
                revision_height: 150,
            },
        };

        let mut data = vec![];
        frozen_client_state.try_serialize(&mut data).unwrap();
        account.data = data;
    }

    let params = UploadMisbehaviourChunkParams {
        client_id: client_id.to_string(),
        chunk_index,
        chunk_data: vec![1u8; 100],
    };

    let instruction = create_upload_instruction(&test_accounts, params);

    let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
    let checks = vec![Check::err(
        anchor_lang::error::Error::from(ErrorCode::ClientFrozen).into(),
    )];
    mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
}

#[test]
fn test_upload_chunk_data_too_large() {
    let client_id = "test-client";
    let chunk_index = 0;
    let submitter = Pubkey::new_unique();

    let test_accounts = setup_test_accounts(client_id, chunk_index, submitter, true);

    let params = UploadMisbehaviourChunkParams {
        client_id: client_id.to_string(),
        chunk_index,
        chunk_data: vec![1u8; CHUNK_DATA_SIZE + 1],
    };

    let instruction = create_upload_instruction(&test_accounts, params);

    let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
    let checks = vec![Check::err(
        anchor_lang::error::Error::from(ErrorCode::ChunkDataTooLarge).into(),
    )];
    mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
}
