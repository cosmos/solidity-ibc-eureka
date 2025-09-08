use crate::error::ErrorCode;
use crate::InitializeUpload;
use anchor_lang::prelude::*;

pub fn initialize_upload(
    ctx: Context<InitializeUpload>,
    chain_id: String,
    target_height: u64,
    total_chunks: u8,
    header_commitment: [u8; 32],
) -> Result<()> {
    let clock = Clock::get()?;
    let metadata = &mut ctx.accounts.metadata;

    require!(total_chunks > 0, ErrorCode::InvalidChunkCount);

    metadata.chain_id = chain_id;
    metadata.target_height = target_height;
    metadata.total_chunks = total_chunks;
    metadata.header_commitment = header_commitment;
    metadata.created_at = clock.unix_timestamp;
    metadata.updated_at = clock.unix_timestamp;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorCode;
    use crate::state::HeaderMetadata;
    use crate::test_helpers::PROGRAM_BINARY_PATH;
    use crate::types::{ClientState, IbcHeight};
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
        metadata_pda: Pubkey,
        client_state_pda: Pubkey,
        accounts: Vec<(Pubkey, Account)>,
    }

    fn setup_test_accounts(
        chain_id: &str,
        target_height: u64,
        submitter: Pubkey,
        with_existing_client: bool,
    ) -> TestAccounts {
        // Derive PDAs
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
            metadata_pda,
            client_state_pda,
            accounts,
        }
    }

    fn create_initialize_upload_instruction(
        test_accounts: &TestAccounts,
        chain_id: String,
        target_height: u64,
        total_chunks: u8,
        header_commitment: [u8; 32],
    ) -> Instruction {
        let instruction_data = crate::instruction::InitializeUpload {
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
    fn test_initialize_metadata_success() {
        let chain_id = "test-chain";
        let target_height = 200;
        let total_chunks = 5;
        let submitter = Pubkey::new_unique();
        let header_commitment = keccak::hash(b"test_header").0;

        let test_accounts = setup_test_accounts(
            chain_id,
            target_height,
            submitter,
            true, // with existing client
        );

        let instruction = create_initialize_upload_instruction(
            &test_accounts,
            chain_id.to_string(),
            target_height,
            total_chunks,
            header_commitment,
        );

        let result = assert_instruction_succeeds(&instruction, &test_accounts.accounts);

        // Verify metadata account was created and populated
        let metadata_account = result
            .resulting_accounts
            .iter()
            .find(|(k, _)| *k == test_accounts.metadata_pda)
            .expect("metadata account should exist");

        assert!(
            metadata_account.1.lamports > 0,
            "metadata should be rent-exempt"
        );
        assert_eq!(
            metadata_account.1.owner,
            crate::ID,
            "metadata should be owned by program"
        );

        // Deserialize and verify metadata
        let metadata: HeaderMetadata =
            HeaderMetadata::try_deserialize(&mut &metadata_account.1.data[..])
                .expect("should deserialize metadata");

        assert_eq!(metadata.chain_id, chain_id);
        assert_eq!(metadata.target_height, target_height);
        assert_eq!(metadata.total_chunks, total_chunks);
        assert_eq!(metadata.header_commitment, header_commitment);
        assert!(metadata.created_at >= 0);
        assert_eq!(metadata.updated_at, metadata.created_at);
    }

    #[test]
    fn test_initialize_metadata_twice_fails() {
        let chain_id = "test-chain";
        let target_height = 200;
        let total_chunks = 3;
        let submitter = Pubkey::new_unique();
        let header_commitment = keccak::hash(b"test_header").0;

        let mut test_accounts = setup_test_accounts(chain_id, target_height, submitter, true);

        // First initialization should succeed
        let instruction = create_initialize_upload_instruction(
            &test_accounts,
            chain_id.to_string(),
            target_height,
            total_chunks,
            header_commitment,
        );

        let result = assert_instruction_succeeds(&instruction, &test_accounts.accounts);
        test_accounts.accounts = result.resulting_accounts.into_iter().collect();

        // Second initialization with same parameters should fail
        let instruction2 = create_initialize_upload_instruction(
            &test_accounts,
            chain_id.to_string(),
            target_height,
            total_chunks,
            header_commitment,
        );

        // This should fail because the account already exists
        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let result2 = mollusk.process_instruction(&instruction2, &test_accounts.accounts);

        assert!(
            !matches!(
                result2.program_result,
                mollusk_svm::result::ProgramResult::Success
            ),
            "Should fail when trying to initialize metadata twice"
        );
    }

    #[test]
    fn test_initialize_metadata_invalid_total_chunks() {
        let chain_id = "test-chain";
        let target_height = 200;
        let total_chunks = 0; // Invalid: must be > 0
        let submitter = Pubkey::new_unique();
        let header_commitment = keccak::hash(b"test_header").0;

        let test_accounts = setup_test_accounts(chain_id, target_height, submitter, true);

        let instruction = create_initialize_upload_instruction(
            &test_accounts,
            chain_id.to_string(),
            target_height,
            total_chunks,
            header_commitment,
        );

        assert_instruction_fails_with_error(
            &instruction,
            &test_accounts.accounts,
            ErrorCode::InvalidChunkCount,
        );
    }

    #[test]
    fn test_initialize_metadata_without_client_fails() {
        let chain_id = "test-chain";
        let target_height = 200;
        let total_chunks = 3;
        let submitter = Pubkey::new_unique();
        let header_commitment = keccak::hash(b"test_header").0;

        let test_accounts = setup_test_accounts(
            chain_id,
            target_height,
            submitter,
            false, // no existing client
        );

        let instruction = create_initialize_upload_instruction(
            &test_accounts,
            chain_id.to_string(),
            target_height,
            total_chunks,
            header_commitment,
        );

        // This should fail because the client doesn't exist
        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let result = mollusk.process_instruction(&instruction, &test_accounts.accounts);

        assert!(
            !matches!(
                result.program_result,
                mollusk_svm::result::ProgramResult::Success
            ),
            "Should fail without client"
        );
    }

    #[test]
    fn test_initialize_metadata_with_different_chain_id_fails() {
        let chain_id = "test-chain";
        let wrong_chain_id = "wrong-chain";
        let target_height = 200;
        let total_chunks = 3;
        let submitter = Pubkey::new_unique();
        let header_commitment = keccak::hash(b"test_header").0;

        let test_accounts = setup_test_accounts(
            chain_id, // Setup with correct chain_id for client
            target_height,
            submitter,
            true,
        );

        // Try to initialize metadata with a different chain_id
        let instruction = create_initialize_upload_instruction(
            &test_accounts,
            wrong_chain_id.to_string(), // Wrong chain_id
            target_height,
            total_chunks,
            header_commitment,
        );

        // This should fail due to constraint violation
        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let result = mollusk.process_instruction(&instruction, &test_accounts.accounts);

        assert!(
            !matches!(
                result.program_result,
                mollusk_svm::result::ProgramResult::Success
            ),
            "Should fail with mismatched chain_id"
        );
    }
}
