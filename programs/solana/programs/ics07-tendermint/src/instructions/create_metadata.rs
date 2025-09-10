use crate::error::ErrorCode;
use crate::CreateMetadata;
use anchor_lang::prelude::*;

pub fn create_metadata(
    ctx: Context<CreateMetadata>,
    chain_id: String,
    target_height: u64,
    total_chunks: u8,
    header_commitment: [u8; 32],
) -> Result<()> {
    require!(total_chunks > 0, ErrorCode::InvalidChunkCount);

    let clock = Clock::get()?;
    let metadata = &mut ctx.accounts.metadata;
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

    impl TestAccounts {
        fn new(target_height: u64) -> Self {
            let submitter = Pubkey::new_unique();
            let chain_id = "test_chain";
            let (metadata_pda, _) = Pubkey::find_program_address(
                &[
                    b"header_metadata",
                    submitter.as_ref(),
                    chain_id.as_bytes(),
                    &target_height.to_le_bytes(),
                ],
                &crate::ID,
            );

            let client_state_pda =
                Pubkey::find_program_address(&[b"client", chain_id.as_bytes()], &crate::ID).0;

            let mut accounts = vec![];

            // Metadata account starts empty (will be created by instruction)
            accounts.push((metadata_pda, Account::new(0, 0, &system_program::ID)));

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

            accounts.push((
                submitter,
                Account::new(1_000_000_000, 0, &system_program::ID),
            ));
            accounts.push(keyed_account_for_system_program());

            Self {
                submitter,
                metadata_pda,
                client_state_pda,
                accounts,
            }
        }
    }

    #[test]
    fn test_create_metadata_success() {
        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let target_height = 100u64;
        let test_accounts = TestAccounts::new(target_height);
        let chain_id = "test_chain".to_string();
        let total_chunks = 5u8;
        let header_commitment = keccak::hash(b"test_header").0;

        let ix_data = crate::instruction::CreateMetadata {
            chain_id: chain_id.clone(),
            target_height,
            total_chunks,
            header_commitment,
        }
        .data();

        let account_metas = vec![
            AccountMeta::new(test_accounts.metadata_pda, false),
            AccountMeta::new_readonly(test_accounts.client_state_pda, false),
            AccountMeta::new(test_accounts.submitter, true),
            AccountMeta::new_readonly(system_program::ID, false),
        ];

        let instruction = Instruction::new_with_bytes(crate::ID, &ix_data, account_metas);

        let result = mollusk.process_instruction(&instruction, &test_accounts.accounts);
        assert!(matches!(
            result.program_result,
            mollusk_svm::result::ProgramResult::Success
        ));

        let metadata_account = result
            .get_account(&test_accounts.metadata_pda)
            .expect("Metadata account should exist after creation");
        let metadata =
            HeaderMetadata::try_deserialize(&mut metadata_account.data.as_slice())
                .expect("Failed to deserialize header metadata");

        assert_eq!(metadata.chain_id, chain_id);
        assert_eq!(metadata.target_height, target_height);
        assert_eq!(metadata.total_chunks, total_chunks);
        assert_eq!(metadata.header_commitment, header_commitment);
        assert!(metadata.created_at >= 0);
        assert_eq!(metadata.created_at, metadata.updated_at);
    }

    #[test]
    fn test_invalid_total_chunks_fails() {
        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let target_height = 100u64;
        let test_accounts = TestAccounts::new(target_height);
        let chain_id = "test_chain".to_string();
        let total_chunks = 0u8; // Invalid
        let header_commitment = keccak::hash(b"test_header").0;

        let ix_data = crate::instruction::CreateMetadata {
            chain_id,
            target_height,
            total_chunks,
            header_commitment,
        }
        .data();

        let account_metas = vec![
            AccountMeta::new(test_accounts.metadata_pda, false),
            AccountMeta::new_readonly(test_accounts.client_state_pda, false),
            AccountMeta::new(test_accounts.submitter, true),
            AccountMeta::new_readonly(system_program::ID, false),
        ];

        let instruction = Instruction::new_with_bytes(crate::ID, &ix_data, account_metas);

        let result = mollusk.process_instruction(&instruction, &test_accounts.accounts);
        assert!(!matches!(
            result.program_result,
            mollusk_svm::result::ProgramResult::Success
        ));
    }
}
