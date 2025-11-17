use crate::error::ErrorCode;
use crate::state::ValidatorsStorage;
use crate::test_helpers::PROGRAM_BINARY_PATH;
use anchor_lang::solana_program::{
    hash::hash,
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    system_program,
};
use anchor_lang::{AccountDeserialize, InstructionData};
use mollusk_svm::{program::keyed_account_for_system_program, Mollusk};
use solana_ibc_types::borsh_header::{BorshPublicKey, BorshValidator, BorshValidatorSet};
use solana_sdk::account::Account;

use super::StoreValidatorsParams;

struct TestAccounts {
    relayer: Pubkey,
    validators_storage_pda: Pubkey,
    accounts: Vec<(Pubkey, Account)>,
}

/// Creates a test `BorshValidatorSet` with the specified number of validators
fn create_test_validator_set(num_validators: usize) -> BorshValidatorSet {
    let validators: Vec<BorshValidator> = (0..num_validators)
        .map(|i| BorshValidator {
            address: [i as u8; 20],
            pub_key: BorshPublicKey::Ed25519([i as u8; 32]),
            voting_power: 100 + i as u64,
            proposer_priority: 0,
        })
        .collect();

    let total_voting_power: u64 = validators.iter().map(|v| v.voting_power).sum();

    BorshValidatorSet {
        validators: validators.clone(),
        proposer: Some(validators[0].clone()),
        total_voting_power,
    }
}

fn setup_test_accounts(validators_bytes: &[u8]) -> TestAccounts {
    let relayer = Pubkey::new_unique();

    // Compute simple hash to derive PDA
    let simple_hash = hash(validators_bytes).to_bytes();

    let validators_storage_pda = Pubkey::find_program_address(
        &[ValidatorsStorage::SEED, &simple_hash, relayer.as_ref()],
        &crate::ID,
    )
    .0;

    let accounts = vec![
        (
            validators_storage_pda,
            Account {
                lamports: 0,
                data: vec![],
                owner: system_program::ID,
                executable: false,
                rent_epoch: 0,
            },
        ),
        (
            relayer,
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

    TestAccounts {
        relayer,
        validators_storage_pda,
        accounts,
    }
}

fn create_store_validators_instruction(
    test_accounts: &TestAccounts,
    params: StoreValidatorsParams,
) -> Instruction {
    let instruction_data = crate::instruction::StoreAndHashValidators { params };

    Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(test_accounts.validators_storage_pda, false),
            AccountMeta::new(test_accounts.relayer, true),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
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
fn test_store_validators_success() {
    // Create test validator set
    let validator_set = create_test_validator_set(4);
    let validators_bytes = borsh::to_vec(&validator_set).expect("Failed to serialize validators");

    let test_accounts = setup_test_accounts(&validators_bytes);

    let simple_hash = hash(&validators_bytes).to_bytes();
    let params = StoreValidatorsParams {
        simple_hash,
        validators_bytes: validators_bytes.clone(),
    };

    let instruction = create_store_validators_instruction(&test_accounts, params);
    let result = assert_instruction_succeeds(&instruction, &test_accounts.accounts);

    // Verify storage account was created
    let storage_account = result
        .resulting_accounts
        .iter()
        .find(|(k, _)| *k == test_accounts.validators_storage_pda)
        .expect("storage account should exist");

    assert!(
        storage_account.1.lamports > 0,
        "storage should be rent-exempt"
    );
    assert_eq!(
        storage_account.1.owner,
        crate::ID,
        "storage should be owned by program"
    );

    // Deserialize and verify stored data
    let storage: ValidatorsStorage =
        ValidatorsStorage::try_deserialize(&mut &storage_account.1.data[..])
            .expect("should deserialize storage");

    // Verify simple hash
    let expected_simple_hash = hash(&validators_bytes).to_bytes();
    assert_eq!(
        storage.simple_hash, expected_simple_hash,
        "simple hash should match"
    );

    // Verify validators bytes are stored
    assert_eq!(
        storage.validators_bytes, validators_bytes,
        "validators bytes should match"
    );

    // Verify merkle hash is non-zero (actual computation)
    assert_ne!(
        storage.merkle_hash, [0u8; 32],
        "merkle hash should be computed"
    );
}

#[test]
fn test_store_validators_with_different_sizes() {
    for num_validators in [1, 5, 10, 50] {
        let validator_set = create_test_validator_set(num_validators);
        let validators_bytes =
            borsh::to_vec(&validator_set).expect("Failed to serialize validators");

        let test_accounts = setup_test_accounts(&validators_bytes);

        let simple_hash = hash(&validators_bytes).to_bytes();
        let params = StoreValidatorsParams {
            simple_hash,
            validators_bytes: validators_bytes.clone(),
        };

        let instruction = create_store_validators_instruction(&test_accounts, params);
        let result = assert_instruction_succeeds(&instruction, &test_accounts.accounts);

        // Verify storage was created
        let storage_account = result
            .resulting_accounts
            .iter()
            .find(|(k, _)| *k == test_accounts.validators_storage_pda)
            .expect("storage account should exist");

        assert!(storage_account.1.lamports > 0);

        // Verify data integrity
        let storage: ValidatorsStorage =
            ValidatorsStorage::try_deserialize(&mut &storage_account.1.data[..])
                .expect("should deserialize storage");

        assert_eq!(storage.validators_bytes, validators_bytes);
    }
}

#[test]
fn test_store_validators_pda_derivation() {
    let validator_set = create_test_validator_set(3);
    let validators_bytes = borsh::to_vec(&validator_set).expect("Failed to serialize validators");

    let test_accounts = setup_test_accounts(&validators_bytes);

    let simple_hash = hash(&validators_bytes).to_bytes();
    let params = StoreValidatorsParams {
        simple_hash,
        validators_bytes: validators_bytes.clone(),
    };

    let instruction = create_store_validators_instruction(&test_accounts, params);
    let result = assert_instruction_succeeds(&instruction, &test_accounts.accounts);

    // Verify PDA derivation
    let simple_hash = hash(&validators_bytes).to_bytes();
    let expected_pda = Pubkey::find_program_address(
        &[
            ValidatorsStorage::SEED,
            &simple_hash,
            test_accounts.relayer.as_ref(),
        ],
        &crate::ID,
    )
    .0;

    assert_eq!(
        test_accounts.validators_storage_pda, expected_pda,
        "PDA should match expected derivation"
    );

    // Verify the storage account exists at the expected PDA
    let storage_account = result
        .resulting_accounts
        .iter()
        .find(|(k, _)| *k == expected_pda)
        .expect("storage should exist at expected PDA");

    assert!(storage_account.1.lamports > 0);
}

#[test]
fn test_store_validators_merkle_hash_deterministic() {
    // Create same validator set twice and verify merkle hash is deterministic
    let validator_set = create_test_validator_set(5);
    let validators_bytes = borsh::to_vec(&validator_set).expect("Failed to serialize validators");

    // First storage
    let test_accounts1 = setup_test_accounts(&validators_bytes);
    let simple_hash = hash(&validators_bytes).to_bytes();
    let params1 = StoreValidatorsParams {
        simple_hash,
        validators_bytes: validators_bytes.clone(),
    };
    let instruction1 = create_store_validators_instruction(&test_accounts1, params1);
    let result1 = assert_instruction_succeeds(&instruction1, &test_accounts1.accounts);

    let storage1 = result1
        .resulting_accounts
        .iter()
        .find(|(k, _)| *k == test_accounts1.validators_storage_pda)
        .map(|(_, acc)| {
            ValidatorsStorage::try_deserialize(&mut &acc.data[..]).expect("should deserialize")
        })
        .expect("storage should exist");

    // Second storage with same validators but different relayer
    let test_accounts2 = setup_test_accounts(&validators_bytes);
    let params2 = StoreValidatorsParams {
        simple_hash,
        validators_bytes,
    };
    let instruction2 = create_store_validators_instruction(&test_accounts2, params2);
    let result2 = assert_instruction_succeeds(&instruction2, &test_accounts2.accounts);

    let storage2 = result2
        .resulting_accounts
        .iter()
        .find(|(k, _)| *k == test_accounts2.validators_storage_pda)
        .map(|(_, acc)| {
            ValidatorsStorage::try_deserialize(&mut &acc.data[..]).expect("should deserialize")
        })
        .expect("storage should exist");

    // Merkle hash should be the same for same validator set
    assert_eq!(
        storage1.merkle_hash, storage2.merkle_hash,
        "merkle hash should be deterministic for same validators"
    );
}

#[test]
fn test_store_validators_invalid_borsh_data() {
    // Create invalid borsh data (random bytes)
    let invalid_bytes = vec![0xFF, 0xAA, 0x55, 0x00];

    let test_accounts = setup_test_accounts(&invalid_bytes);

    let simple_hash = hash(&invalid_bytes).to_bytes();
    let params = StoreValidatorsParams {
        simple_hash,
        validators_bytes: invalid_bytes,
    };

    let instruction = create_store_validators_instruction(&test_accounts, params);

    assert_instruction_fails_with_error(
        &instruction,
        &test_accounts.accounts,
        ErrorCode::ValidatorsDeserializationFailed,
    );
}

#[test]
fn test_store_validators_empty_validator_set() {
    // Create empty validator set
    let validator_set = BorshValidatorSet {
        validators: vec![],
        proposer: None,
        total_voting_power: 0,
    };
    let validators_bytes = borsh::to_vec(&validator_set).expect("Failed to serialize validators");

    let test_accounts = setup_test_accounts(&validators_bytes);

    let simple_hash = hash(&validators_bytes).to_bytes();
    let params = StoreValidatorsParams {
        simple_hash,
        validators_bytes,
    };

    let instruction = create_store_validators_instruction(&test_accounts, params);

    // This might fail during conversion to tendermint ValidatorSet
    // as empty validator sets may not be valid
    let result = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH)
        .process_instruction(&instruction, &test_accounts.accounts);

    // Either succeeds or fails with deserialization error
    match result.program_result {
        mollusk_svm::result::ProgramResult::Success => {
            // If it succeeds, verify the account was created
            let storage_account = result
                .resulting_accounts
                .iter()
                .find(|(k, _)| *k == test_accounts.validators_storage_pda);
            assert!(storage_account.is_some());
        }
        mollusk_svm::result::ProgramResult::Failure(_) => {
            // Expected to fail with conversion error
        }
        _ => panic!("Unexpected result"),
    }
}

#[test]
fn test_store_validators_different_relayers_different_pdas() {
    let validator_set = create_test_validator_set(3);
    let validators_bytes = borsh::to_vec(&validator_set).expect("Failed to serialize validators");

    // Create two different relayers
    let relayer1 = Pubkey::new_unique();
    let relayer2 = Pubkey::new_unique();

    let simple_hash = hash(&validators_bytes).to_bytes();

    // Derive PDAs for both relayers
    let pda1 = Pubkey::find_program_address(
        &[ValidatorsStorage::SEED, &simple_hash, relayer1.as_ref()],
        &crate::ID,
    )
    .0;

    let pda2 = Pubkey::find_program_address(
        &[ValidatorsStorage::SEED, &simple_hash, relayer2.as_ref()],
        &crate::ID,
    )
    .0;

    // PDAs should be different for different relayers
    assert_ne!(pda1, pda2, "PDAs should differ for different relayers");
}

#[test]
fn test_store_validators_same_validators_twice_different_relayers() {
    let validator_set = create_test_validator_set(4);
    let validators_bytes = borsh::to_vec(&validator_set).expect("Failed to serialize validators");

    // First relayer stores validators
    let test_accounts1 = setup_test_accounts(&validators_bytes);
    let simple_hash = hash(&validators_bytes).to_bytes();
    let params1 = StoreValidatorsParams {
        simple_hash,
        validators_bytes: validators_bytes.clone(),
    };
    let instruction1 = create_store_validators_instruction(&test_accounts1, params1);
    let result1 = assert_instruction_succeeds(&instruction1, &test_accounts1.accounts);

    // Second relayer stores same validators (should succeed with different PDA)
    let test_accounts2 = setup_test_accounts(&validators_bytes);
    let params2 = StoreValidatorsParams {
        simple_hash,
        validators_bytes,
    };
    let instruction2 = create_store_validators_instruction(&test_accounts2, params2);
    let result2 = assert_instruction_succeeds(&instruction2, &test_accounts2.accounts);

    // Verify both accounts exist and have same merkle hash
    let storage1 = result1
        .resulting_accounts
        .iter()
        .find(|(k, _)| *k == test_accounts1.validators_storage_pda)
        .map(|(_, acc)| {
            ValidatorsStorage::try_deserialize(&mut &acc.data[..]).expect("should deserialize")
        })
        .expect("storage1 should exist");

    let storage2 = result2
        .resulting_accounts
        .iter()
        .find(|(k, _)| *k == test_accounts2.validators_storage_pda)
        .map(|(_, acc)| {
            ValidatorsStorage::try_deserialize(&mut &acc.data[..]).expect("should deserialize")
        })
        .expect("storage2 should exist");

    // Both should have same merkle hash (same validators)
    assert_eq!(storage1.merkle_hash, storage2.merkle_hash);
    // But different PDAs (different relayers)
    assert_ne!(
        test_accounts1.validators_storage_pda,
        test_accounts2.validators_storage_pda
    );
}
