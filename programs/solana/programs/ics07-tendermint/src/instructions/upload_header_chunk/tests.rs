use crate::error::ErrorCode;
use crate::state::{HeaderChunk, HeaderMetadata, CHUNK_DATA_SIZE};
use crate::test_helpers::{fixtures::assert_error_code, PROGRAM_BINARY_PATH};
use crate::types::{ClientState, IbcHeight, UploadChunkParams};
use anchor_lang::solana_program::{
    instruction::{AccountMeta, Instruction},
    keccak,
    pubkey::Pubkey,
    system_program,
};
use anchor_lang::{AccountDeserialize, AccountSerialize, InstructionData};
use mollusk_svm::{program::keyed_account_for_system_program, Mollusk};
use solana_sdk::account::Account;

struct TestAccounts {
    submitter: Pubkey,
    chunk_pda: Pubkey,
    metadata_pda: Pubkey,
    client_state_pda: Pubkey,
    accounts: Vec<(Pubkey, Account)>,
}

fn setup_test_accounts(
    chain_id: &str,
    target_height: u64,
    chunk_index: u8,
    submitter: Pubkey,
    with_existing_client: bool,
) -> TestAccounts {
    // Derive PDAs
    let chunk_pda = Pubkey::find_program_address(
        &[
            b"header_chunk",
            submitter.as_ref(),
            chain_id.as_bytes(),
            &target_height.to_le_bytes(),
            &[chunk_index],
        ],
        &crate::ID,
    )
    .0;

    let metadata_pda = Pubkey::find_program_address(
        &[
            b"header_metadata",
            submitter.as_ref(),
            chain_id.as_bytes(),
            &target_height.to_le_bytes(),
        ],
        &crate::ID,
    )
    .0;

    let client_state_pda =
        Pubkey::find_program_address(&[b"client", chain_id.as_bytes()], &crate::ID).0;

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
            metadata_pda,
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
        // Create client state account
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
                revision_height: 100,
            },
        };

        let mut client_data = vec![];
        client_state.try_serialize(&mut client_data).unwrap();

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
    } else {
        accounts.push((
            client_state_pda,
            Account {
                lamports: 0,
                data: vec![],
                owner: system_program::ID,
                executable: false,
                rent_epoch: 0,
            },
        ));
    }

    TestAccounts {
        submitter,
        chunk_pda,
        metadata_pda,
        client_state_pda,
        accounts,
    }
}

fn create_upload_chunk_params(
    chain_id: &str,
    target_height: u64,
    chunk_index: u8,
    chunk_data: Vec<u8>,
) -> UploadChunkParams {
    let chunk_hash = keccak::hash(&chunk_data).0;

    UploadChunkParams {
        chain_id: chain_id.to_string(),
        target_height,
        chunk_index,
        chunk_data,
        chunk_hash,
    }
}

fn create_or_update_metadata_instruction(
    test_accounts: &TestAccounts,
    chain_id: String,
    target_height: u64,
    total_chunks: u8,
    header_commitment: [u8; 32],
) -> Instruction {
    let instruction_data = crate::instruction::CreateMetadata {
        chain_id,
        target_height,
        total_chunks,
        header_commitment,
    };

    Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(test_accounts.metadata_pda, false),
            AccountMeta::new_readonly(test_accounts.client_state_pda, false),
            AccountMeta::new(test_accounts.submitter, true),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
        data: instruction_data.data(),
    }
}

fn create_upload_instruction(
    test_accounts: &TestAccounts,
    params: UploadChunkParams,
) -> Instruction {
    let instruction_data = crate::instruction::UploadHeaderChunk { params };

    Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(test_accounts.chunk_pda, false),
            AccountMeta::new(test_accounts.metadata_pda, false),
            AccountMeta::new_readonly(test_accounts.client_state_pda, false),
            AccountMeta::new(test_accounts.submitter, true),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
        data: instruction_data.data(),
    }
}

fn initialize_metadata(
    test_accounts: &TestAccounts,
    chain_id: &str,
    target_height: u64,
    total_chunks: u8,
    header_commitment: [u8; 32],
) -> Vec<(Pubkey, Account)> {
    let instruction = create_or_update_metadata_instruction(
        test_accounts,
        chain_id.to_string(),
        target_height,
        total_chunks,
        header_commitment,
    );

    let result = assert_instruction_succeeds(&instruction, &test_accounts.accounts);
    result.resulting_accounts.into_iter().collect()
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

fn assert_instruction_fails_with_error(
    instruction: &Instruction,
    accounts: &[(Pubkey, Account)],
    expected_error: ErrorCode,
) {
    let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
    let result = mollusk.process_instruction(instruction, accounts);

    match result.program_result {
        mollusk_svm::result::ProgramResult::Success => {
            panic!("Expected instruction to fail with {expected_error:?}, but it succeeded");
        }
        mollusk_svm::result::ProgramResult::Failure(error) => {
            assert_eq!(
                error,
                anchor_lang::error::Error::from(expected_error).into()
            );
        }
        mollusk_svm::result::ProgramResult::UnknownError(error) => {
            panic!("Unknown error occurred: {error:?}");
        }
    }
}

#[test]
fn test_upload_first_chunk_success() {
    let chain_id = "test-chain";
    let target_height = 200;
    let chunk_index = 0;
    let total_chunks = 3;
    let submitter = Pubkey::new_unique();

    let mut test_accounts = setup_test_accounts(
        chain_id,
        target_height,
        chunk_index,
        submitter,
        true, // with existing client
    );

    let chunk_data = vec![1u8; 100];
    let header_commitment = keccak::hash(&chunk_data).0; // Compute before moving chunk_data
    let params = create_upload_chunk_params(
        chain_id,
        target_height,
        chunk_index,
        chunk_data,
    );

    let expected_hash = params.chunk_hash;
    let expected_data = params.chunk_data.clone();

    // First initialize the metadata
    test_accounts.accounts = initialize_metadata(
        &test_accounts,
        chain_id,
        target_height,
        total_chunks,
        header_commitment,
    );

    // Then upload the chunk
    let instruction = create_upload_instruction(&test_accounts, params);
    let result = assert_instruction_succeeds(&instruction, &test_accounts.accounts);

    // Verify chunk account was created and populated
    let chunk_account = result
        .resulting_accounts
        .iter()
        .find(|(k, _)| *k == test_accounts.chunk_pda)
        .expect("chunk account should exist");

    assert!(chunk_account.1.lamports > 0, "chunk should be rent-exempt");
    assert_eq!(
        chunk_account.1.owner,
        crate::ID,
        "chunk should be owned by program"
    );

    // Deserialize and verify chunk data
    let chunk: HeaderChunk = HeaderChunk::try_deserialize(&mut &chunk_account.1.data[..])
        .expect("should deserialize chunk");

    assert_eq!(chunk.chain_id, chain_id);
    assert_eq!(chunk.target_height, target_height);
    assert_eq!(chunk.chunk_index, chunk_index);
    assert_eq!(chunk.chunk_hash, expected_hash);
    assert_eq!(chunk.chunk_data, expected_data);
    assert_eq!(chunk.version, 1);
}

#[test]
fn test_upload_chunk_with_invalid_hash_fails() {
    let chain_id = "test-chain";
    let target_height = 200;
    let chunk_index = 0;
    let submitter = Pubkey::new_unique();

    let mut test_accounts =
        setup_test_accounts(chain_id, target_height, chunk_index, submitter, true);

    let mut params =
        create_upload_chunk_params(chain_id, target_height, chunk_index, vec![1u8; 100]);

    // Compute the header commitment
    let header_commitment = keccak::hash(&params.chunk_data).0;
    
    // Initialize metadata first
    test_accounts.accounts = initialize_metadata(
        &test_accounts,
        chain_id,
        target_height,
        3,
        header_commitment,
    );
    
    // Corrupt the hash
    params.chunk_hash = [0u8; 32];

    let instruction = create_upload_instruction(&test_accounts, params);
    assert_instruction_fails_with_error(
        &instruction,
        &test_accounts.accounts,
        ErrorCode::InvalidChunkHash,
    );
}

#[test]
fn test_upload_same_chunk_twice_with_same_hash() {
    let chain_id = "test-chain";
    let target_height = 200;
    let chunk_index = 0;
    let submitter = Pubkey::new_unique();

    let mut test_accounts =
        setup_test_accounts(chain_id, target_height, chunk_index, submitter, true);

    let chunk_data = vec![1u8; 100];
    let header_commitment = keccak::hash(&chunk_data).0;
    let params = create_upload_chunk_params(chain_id, target_height, chunk_index, chunk_data);

    // Initialize metadata first
    test_accounts.accounts = initialize_metadata(
        &test_accounts,
        chain_id,
        target_height,
        3,
        header_commitment,
    );

    // First upload
    let instruction = create_upload_instruction(&test_accounts, params.clone());
    let result = assert_instruction_succeeds(&instruction, &test_accounts.accounts);

    // Update accounts with results from first upload
    test_accounts.accounts = result.resulting_accounts.into_iter().collect();

    // Second upload with same data (should succeed and increment version)
    let instruction2 = create_upload_instruction(&test_accounts, params);
    let result2 = assert_instruction_succeeds(&instruction2, &test_accounts.accounts);

    // Verify chunk was re-uploaded (version should be 2 now)
    let chunk_account = result2
        .resulting_accounts
        .iter()
        .find(|(k, _)| *k == test_accounts.chunk_pda)
        .expect("chunk account should exist");

    let chunk: HeaderChunk = HeaderChunk::try_deserialize(&mut &chunk_account.1.data[..])
        .expect("should deserialize chunk");

    assert_eq!(
        chunk.version, 2,
        "version should increment even for same data"
    );
}

#[test]
fn test_upload_chunk_overwrites_with_different_data() {
    let chain_id = "test-chain";
    let target_height = 200;
    let chunk_index = 0;
    let submitter = Pubkey::new_unique();

    let mut test_accounts =
        setup_test_accounts(chain_id, target_height, chunk_index, submitter, true);

    // First upload
    let chunk_data1 = vec![1u8; 100];
    let params1 = create_upload_chunk_params(chain_id, target_height, chunk_index, chunk_data1);

    // Save the header commitment before params1 is moved
    let header_commitment = keccak::hash(&params1.chunk_data).0;

    // Initialize metadata first (using params1's header commitment)
    test_accounts.accounts = initialize_metadata(
        &test_accounts,
        chain_id,
        target_height,
        3,
        header_commitment,
    );

    let instruction = create_upload_instruction(&test_accounts, params1);
    let result = assert_instruction_succeeds(&instruction, &test_accounts.accounts);

    // Update accounts with results from first upload
    test_accounts.accounts = result.resulting_accounts.into_iter().collect();

    // Second upload with different data but same header commitment
    // (simulating a re-upload scenario where the full header is the same)
    let chunk_data2 = vec![2u8; 100];
    let params2 =
        create_upload_chunk_params(chain_id, target_height, chunk_index, chunk_data2.clone());

    // params2 already has its own chunk data and hash, no need to set header_commitment

    let instruction2 = create_upload_instruction(&test_accounts, params2);
    let result2 = assert_instruction_succeeds(&instruction2, &test_accounts.accounts);

    // Verify chunk was overwritten
    let chunk_account = result2
        .resulting_accounts
        .iter()
        .find(|(k, _)| *k == test_accounts.chunk_pda)
        .expect("chunk account should exist");

    let chunk: HeaderChunk = HeaderChunk::try_deserialize(&mut &chunk_account.1.data[..])
        .expect("should deserialize chunk");

    assert_eq!(chunk.chunk_data, chunk_data2);
    assert_eq!(chunk.version, 2, "version should increment on overwrite");
}

#[test]
fn test_upload_multiple_chunks_creates_shared_metadata() {
    let chain_id = "test-chain";
    let target_height = 200;
    let submitter = Pubkey::new_unique();
    let total_chunks = 3;

    // Upload chunk 0
    let mut test_accounts0 = setup_test_accounts(chain_id, target_height, 0, submitter, true);

    let params0 =
        create_upload_chunk_params(chain_id, target_height, 0, vec![1u8; 100]);

    // For this test, we'll use a commitment that represents the full header
    // In a real scenario, this would be computed from all chunks combined
    let expected_commitment = keccak::hash(b"full_header_data").0;

    // Initialize metadata first
    test_accounts0.accounts = initialize_metadata(
        &test_accounts0,
        chain_id,
        target_height,
        total_chunks,
        expected_commitment,
    );

    let instruction0 = create_upload_instruction(&test_accounts0, params0);
    let result0 = assert_instruction_succeeds(&instruction0, &test_accounts0.accounts);

    // Get metadata from first upload
    let metadata_account = result0
        .resulting_accounts
        .iter()
        .find(|(k, _)| *k == test_accounts0.metadata_pda)
        .expect("metadata account should exist");

    let metadata: HeaderMetadata =
        HeaderMetadata::try_deserialize(&mut &metadata_account.1.data[..])
            .expect("should deserialize metadata");

    assert_eq!(metadata.chain_id, chain_id);
    assert_eq!(metadata.target_height, target_height);
    assert_eq!(metadata.total_chunks, total_chunks);
    assert_eq!(metadata.header_commitment, expected_commitment);
    // In tests, Clock might return 0, so just check it was set
    assert!(metadata.created_at >= 0);

    // Upload chunk 1 (should use same metadata)
    test_accounts0.accounts = result0.resulting_accounts.into_iter().collect();

    let test_accounts1 = setup_test_accounts(chain_id, target_height, 1, submitter, true);

    // Update accounts to include existing metadata from chunk 0
    let mut accounts1 = test_accounts0.accounts.clone();
    accounts1.retain(|(k, _)| *k != test_accounts1.chunk_pda);
    accounts1.push((
        test_accounts1.chunk_pda,
        Account {
            lamports: 0,
            data: vec![],
            owner: system_program::ID,
            executable: false,
            rent_epoch: 0,
        },
    ));

    let params1 =
        create_upload_chunk_params(chain_id, target_height, 1, vec![1u8; 100]);

    let instruction1 = create_upload_instruction(&test_accounts1, params1);
    let result1 = assert_instruction_succeeds(&instruction1, &accounts1);

    // Verify metadata wasn't changed
    let metadata_account = result1
        .resulting_accounts
        .iter()
        .find(|(k, _)| *k == test_accounts1.metadata_pda)
        .expect("metadata account should exist");

    let metadata2: HeaderMetadata =
        HeaderMetadata::try_deserialize(&mut &metadata_account.1.data[..])
            .expect("should deserialize metadata");

    assert_eq!(metadata2.header_commitment, metadata.header_commitment);
    assert_eq!(metadata2.total_chunks, metadata.total_chunks);
}

#[test]
fn test_upload_chunk_without_metadata_fails() {
    let chain_id = "test-chain";
    let target_height = 200;
    let chunk_index = 0;
    let submitter = Pubkey::new_unique();

    let test_accounts = setup_test_accounts(
        chain_id,
        target_height,
        chunk_index,
        submitter,
        true, // with existing client
    );

    let params =
        create_upload_chunk_params(chain_id, target_height, chunk_index, vec![1u8; 100]);

    // Try to upload chunk without initializing metadata first

    let instruction = create_upload_instruction(&test_accounts, params);
    // This should fail because the metadata doesn't exist
    let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
    let result = mollusk.process_instruction(&instruction, &test_accounts.accounts);

    // Anchor's constraint validation happens first when metadata doesn't exist
    assert!(result.program_result.is_err()); // Error 3012 is ConstraintAccountIsNone
}

#[test]
fn test_upload_chunk_exceeding_max_size_fails() {
    let chain_id = "test-chain";
    let target_height = 200;
    let chunk_index = 0;
    let submitter = Pubkey::new_unique();

    let mut test_accounts =
        setup_test_accounts(chain_id, target_height, chunk_index, submitter, true);

    // Create chunk data that exceeds max size
    let oversized_data = vec![1u8; CHUNK_DATA_SIZE + 1];

    let params =
        create_upload_chunk_params(chain_id, target_height, chunk_index, oversized_data);
    
    let header_commitment = keccak::hash(&params.chunk_data).0;

    // Initialize metadata first
    test_accounts.accounts = initialize_metadata(
        &test_accounts,
        chain_id,
        target_height,
        3,
        header_commitment,
    );

    let instruction = create_upload_instruction(&test_accounts, params);

    assert_instruction_fails_with_error(
        &instruction,
        &test_accounts.accounts,
        ErrorCode::ChunkDataTooLarge,
    );
}
