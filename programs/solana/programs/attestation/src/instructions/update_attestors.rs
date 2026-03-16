use crate::error::ErrorCode;
use crate::events::AttestorsUpdated;
use crate::types::{AppState, ClientState, MAX_ATTESTORS};
use crate::ETH_ADDRESS_LEN;
use anchor_lang::prelude::*;

/// Updates the attestor address set and signature threshold. Requires admin authorization.
#[derive(Accounts)]
#[instruction(attestor_addresses: Vec<[u8; ETH_ADDRESS_LEN]>, min_required_sigs: u8)]
pub struct UpdateAttestors<'info> {
    /// The attestation client state PDA whose attestor set will be updated.
    #[account(
        mut,
        seeds = [ClientState::SEED],
        bump
    )]
    pub client_state: Account<'info, ClientState>,

    /// The attestation app state PDA (read-only, for access manager reference).
    #[account(
        seeds = [AppState::SEED],
        bump
    )]
    pub app_state: Account<'info, AppState>,

    /// The current access manager account for admin verification.
    /// CHECK: Validated via seeds constraint using the stored `access_manager` program ID
    #[account(
        seeds = [access_manager::state::AccessManager::SEED],
        bump,
        seeds::program = app_state.access_manager
    )]
    pub access_manager: AccountInfo<'info>,

    /// The admin signer authorizing this change.
    pub admin: Signer<'info>,

    /// Instructions sysvar for CPI validation.
    /// CHECK: Address constraint verifies this is the instructions sysvar
    #[account(address = anchor_lang::solana_program::sysvar::instructions::ID)]
    pub instructions_sysvar: AccountInfo<'info>,
}

pub fn update_attestors(
    ctx: Context<UpdateAttestors>,
    attestor_addresses: Vec<[u8; ETH_ADDRESS_LEN]>,
    min_required_sigs: u8,
) -> Result<()> {
    access_manager::require_admin(
        &ctx.accounts.access_manager,
        &ctx.accounts.admin,
        &ctx.accounts.instructions_sysvar,
        &crate::ID,
    )?;

    require!(!attestor_addresses.is_empty(), ErrorCode::NoAttestors);
    require!(
        attestor_addresses.len() <= MAX_ATTESTORS,
        ErrorCode::TooManyAttestors
    );
    require!(
        min_required_sigs > 0 && (min_required_sigs as usize) <= attestor_addresses.len(),
        ErrorCode::BadQuorum
    );

    let mut attestor_addresses = attestor_addresses;
    let original_len = attestor_addresses.len();
    attestor_addresses.sort_unstable();
    attestor_addresses.dedup();
    require!(
        attestor_addresses.len() == original_len,
        ErrorCode::DuplicateSigner
    );

    let client_state = &mut ctx.accounts.client_state;

    let old_attestor_addresses = client_state.attestor_addresses.clone();
    let old_min_required_sigs = client_state.min_required_sigs;

    client_state
        .attestor_addresses
        .clone_from(&attestor_addresses);
    client_state.min_required_sigs = min_required_sigs;

    emit!(AttestorsUpdated {
        old_attestor_addresses,
        old_min_required_sigs,
        new_attestor_addresses: attestor_addresses,
        new_min_required_sigs: min_required_sigs,
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::access_control::create_access_manager_account;
    use crate::test_helpers::accounts::{
        create_app_state_account, create_instructions_sysvar_account, create_payer_account,
    };
    use crate::test_helpers::fixtures::create_test_client_state;
    use crate::test_helpers::PROGRAM_BINARY_PATH;
    use access_manager::AccessManagerError;
    use anchor_lang::InstructionData;
    use mollusk_svm::result::Check;
    use mollusk_svm::Mollusk;
    use solana_sdk::instruction::{AccountMeta, Instruction};
    use solana_sdk::pubkey::Pubkey;

    const ANCHOR_ERROR_OFFSET: u32 = 6000;

    struct TestAccounts {
        admin: Pubkey,
        client_state_pda: Pubkey,
        app_state_pda: Pubkey,
        access_manager_pda: Pubkey,
        accounts: Vec<(Pubkey, solana_sdk::account::Account)>,
    }

    /// Creates a client state account with full `INIT_SPACE` allocation so
    /// the attestor set can grow up to `MAX_ATTESTORS` during updates.
    fn create_full_size_client_state_account(
        client_state: &ClientState,
    ) -> solana_sdk::account::Account {
        use anchor_lang::{AccountSerialize, Space};

        let full_space = 8 + ClientState::INIT_SPACE;
        let mut data = Vec::with_capacity(full_space);
        client_state.try_serialize(&mut data).unwrap();
        data.resize(full_space, 0);

        solana_sdk::account::Account {
            lamports: 10_000_000,
            data,
            owner: crate::ID,
            executable: false,
            rent_epoch: u64::MAX,
        }
    }

    fn setup_test_accounts() -> TestAccounts {
        let admin = Pubkey::new_unique();
        let client_state_pda = ClientState::pda();
        let app_state_pda = AppState::pda();

        let initial_client_state =
            create_test_client_state(vec![[1u8; 20], [2u8; 20], [3u8; 20]], 2, 100);

        let (access_manager_pda, access_manager_account) =
            create_access_manager_account(admin, vec![]);

        let (instructions_sysvar_pubkey, instructions_sysvar_account) =
            create_instructions_sysvar_account();

        let accounts = vec![
            (
                client_state_pda,
                create_full_size_client_state_account(&initial_client_state),
            ),
            (app_state_pda, create_app_state_account(access_manager::ID)),
            (access_manager_pda, access_manager_account),
            (admin, create_payer_account()),
            (instructions_sysvar_pubkey, instructions_sysvar_account),
        ];

        TestAccounts {
            admin,
            client_state_pda,
            app_state_pda,
            access_manager_pda,
            accounts,
        }
    }

    fn create_update_attestors_instruction(
        test_accounts: &TestAccounts,
        attestor_addresses: Vec<[u8; ETH_ADDRESS_LEN]>,
        min_required_sigs: u8,
    ) -> Instruction {
        let instruction_data = crate::instruction::UpdateAttestors {
            attestor_addresses,
            min_required_sigs,
        };

        Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(test_accounts.client_state_pda, false),
                AccountMeta::new_readonly(test_accounts.app_state_pda, false),
                AccountMeta::new_readonly(test_accounts.access_manager_pda, false),
                AccountMeta::new_readonly(test_accounts.admin, true),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
            data: instruction_data.data(),
        }
    }

    fn expect_success(test_accounts: &TestAccounts, instruction: Instruction) {
        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        mollusk.process_and_validate_instruction(
            &instruction,
            &test_accounts.accounts,
            &[Check::success()],
        );
    }

    #[test]
    fn test_update_attestors_success() {
        let test_accounts = setup_test_accounts();
        let new_attestors = vec![[10u8; 20], [20u8; 20]];
        let instruction =
            create_update_attestors_instruction(&test_accounts, new_attestors.clone(), 1);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let result = mollusk.process_and_validate_instruction(
            &instruction,
            &test_accounts.accounts,
            &[Check::success()],
        );

        let client_state_account = result
            .get_account(&test_accounts.client_state_pda)
            .expect("Client state account not found");
        let client_state: ClientState =
            ClientState::try_deserialize(&mut &client_state_account.data[..])
                .expect("Failed to deserialize client state");

        let mut expected = new_attestors;
        expected.sort_unstable();
        assert_eq!(client_state.attestor_addresses, expected);
        assert_eq!(client_state.min_required_sigs, 1);
        // Unchanged fields
        assert_eq!(client_state.latest_height, 100);
        assert!(!client_state.is_frozen);
    }

    #[rstest::rstest]
    #[case::single_attestor(vec![[0xAB; 20]], 1)]
    #[case::min_sigs_equals_count(vec![[1u8; 20], [2u8; 20], [3u8; 20]], 3)]
    #[case::max_attestors((0u8..20).map(|i| [i; 20]).collect::<Vec<_>>(), 10)]
    #[case::max_min_required_sigs((0u8..5).map(|i| [i; 20]).collect::<Vec<_>>(), 5)]
    fn test_update_attestors_success_cases(
        #[case] attestors: Vec<[u8; ETH_ADDRESS_LEN]>,
        #[case] min_sigs: u8,
    ) {
        let test_accounts = setup_test_accounts();
        let instruction = create_update_attestors_instruction(&test_accounts, attestors, min_sigs);
        expect_success(&test_accounts, instruction);
    }

    #[test]
    fn test_update_attestors_on_frozen_client() {
        let admin = Pubkey::new_unique();
        let client_state_pda = ClientState::pda();
        let app_state_pda = AppState::pda();

        let frozen_client_state = ClientState {
            version: crate::types::AccountVersion::V1,
            attestor_addresses: vec![[1u8; 20], [2u8; 20]],
            min_required_sigs: 2,
            latest_height: 100,
            is_frozen: true,
        };

        let (access_manager_pda, access_manager_account) =
            create_access_manager_account(admin, vec![]);

        let (instructions_sysvar_pubkey, instructions_sysvar_account) =
            create_instructions_sysvar_account();

        let test_accounts = TestAccounts {
            admin,
            client_state_pda,
            app_state_pda,
            access_manager_pda,
            accounts: vec![
                (
                    client_state_pda,
                    create_full_size_client_state_account(&frozen_client_state),
                ),
                (app_state_pda, create_app_state_account(access_manager::ID)),
                (access_manager_pda, access_manager_account),
                (admin, create_payer_account()),
                (instructions_sysvar_pubkey, instructions_sysvar_account),
            ],
        };

        let instruction = create_update_attestors_instruction(&test_accounts, vec![[10u8; 20]], 1);
        expect_success(&test_accounts, instruction);
    }

    #[rstest::rstest]
    #[case::empty_attestors(vec![], 1, ErrorCode::NoAttestors)]
    #[case::zero_min_sigs(vec![[1u8; 20]], 0, ErrorCode::BadQuorum)]
    #[case::min_sigs_exceeds_attestors(vec![[1u8; 20]], 2, ErrorCode::BadQuorum)]
    #[case::duplicate_attestors(vec![[1u8; 20], [2u8; 20], [1u8; 20]], 2, ErrorCode::DuplicateSigner)]
    #[case::duplicate_attestors_adjacent(vec![[5u8; 20], [5u8; 20]], 1, ErrorCode::DuplicateSigner)]
    #[case::multiple_duplicate_groups(vec![[1u8; 20], [2u8; 20], [1u8; 20], [2u8; 20]], 2, ErrorCode::DuplicateSigner)]
    #[case::too_many_attestors((0u8..21).map(|i| [i; 20]).collect::<Vec<_>>(), 10, ErrorCode::TooManyAttestors)]
    fn test_update_attestors_validation_error(
        #[case] attestors: Vec<[u8; ETH_ADDRESS_LEN]>,
        #[case] min_sigs: u8,
        #[case] expected_error: ErrorCode,
    ) {
        let test_accounts = setup_test_accounts();
        let instruction = create_update_attestors_instruction(&test_accounts, attestors, min_sigs);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(
            anchor_lang::error::Error::from(expected_error).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_update_attestors_non_admin_rejected() {
        let admin = Pubkey::new_unique();
        let non_admin = Pubkey::new_unique();
        let client_state_pda = ClientState::pda();
        let app_state_pda = AppState::pda();

        let initial_client_state =
            create_test_client_state(vec![[1u8; 20], [2u8; 20], [3u8; 20]], 2, 100);

        let (access_manager_pda, access_manager_account) =
            create_access_manager_account(admin, vec![]);

        let (instructions_sysvar_pubkey, instructions_sysvar_account) =
            create_instructions_sysvar_account();

        let instruction_data = crate::instruction::UpdateAttestors {
            attestor_addresses: vec![[10u8; 20]],
            min_required_sigs: 1,
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(client_state_pda, false),
                AccountMeta::new_readonly(app_state_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new_readonly(non_admin, true),
                AccountMeta::new_readonly(instructions_sysvar_pubkey, false),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            (
                client_state_pda,
                create_full_size_client_state_account(&initial_client_state),
            ),
            (app_state_pda, create_app_state_account(access_manager::ID)),
            (access_manager_pda, access_manager_account),
            (non_admin, create_payer_account()),
            (instructions_sysvar_pubkey, instructions_sysvar_account),
        ];

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(solana_sdk::program_error::ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + AccessManagerError::Unauthorized as u32,
        ))];
        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }
}

#[cfg(test)]
mod integration_tests {
    use crate::test_helpers::*;
    use crate::types::ClientState;
    use anchor_lang::AccountDeserialize;
    use anchor_lang::InstructionData;
    use solana_sdk::{
        instruction::{AccountMeta, Instruction},
        pubkey::Pubkey,
        signature::Keypair,
        signer::Signer,
    };

    fn build_update_attestors_ix(
        admin: Pubkey,
        attestor_addresses: Vec<[u8; crate::ETH_ADDRESS_LEN]>,
        min_required_sigs: u8,
    ) -> Instruction {
        let client_state_pda = crate::types::ClientState::pda();
        let app_state_pda = crate::types::AppState::pda();
        let (access_manager_pda, _) = Pubkey::find_program_address(
            &[access_manager::state::AccessManager::SEED],
            &access_manager::ID,
        );

        Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(client_state_pda, false),
                AccountMeta::new_readonly(app_state_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new_readonly(admin, true),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
            data: crate::instruction::UpdateAttestors {
                attestor_addresses,
                min_required_sigs,
            }
            .data(),
        }
    }

    fn setup_program_test_with_client_state(
        admin: &Pubkey,
        whitelisted_programs: &[Pubkey],
    ) -> solana_program_test::ProgramTest {
        use anchor_lang::{AccountSerialize, Space};

        let mut pt = setup_program_test_with_whitelist(admin, whitelisted_programs);

        let client_state_pda = crate::types::ClientState::pda();
        let client_state = ClientState {
            version: crate::types::AccountVersion::V1,
            attestor_addresses: vec![[1u8; 20], [2u8; 20]],
            min_required_sigs: 1,
            latest_height: 50,
            is_frozen: false,
        };
        let full_space = 8 + ClientState::INIT_SPACE;
        let mut data = Vec::with_capacity(full_space);
        client_state.try_serialize(&mut data).unwrap();
        data.resize(full_space, 0);

        pt.add_account(
            client_state_pda,
            solana_sdk::account::Account {
                lamports: 10_000_000,
                data,
                owner: crate::ID,
                executable: false,
                rent_epoch: 0,
            },
        );

        pt
    }

    #[tokio::test]
    async fn test_direct_call_by_admin_succeeds() {
        let admin = Keypair::new();
        let pt = setup_program_test_with_client_state(&admin.pubkey(), &[TEST_CPI_TARGET_ID]);
        let (banks_client, payer, recent_blockhash) = pt.start().await;

        let new_attestors = vec![[10u8; 20], [20u8; 20], [30u8; 20]];
        let ix = build_update_attestors_ix(admin.pubkey(), new_attestors.clone(), 2);

        let tx = solana_sdk::transaction::Transaction::new_signed_with_payer(
            &[ix],
            Some(&payer.pubkey()),
            &[&payer, &admin],
            recent_blockhash,
        );
        let result = banks_client.process_transaction(tx).await;
        assert!(result.is_ok(), "Direct call by admin should succeed");

        let client_state_pda = crate::types::ClientState::pda();
        let account = banks_client
            .get_account(client_state_pda)
            .await
            .unwrap()
            .unwrap();
        let client_state = ClientState::try_deserialize(&mut &account.data[..]).unwrap();

        let mut expected = new_attestors;
        expected.sort_unstable();
        assert_eq!(client_state.attestor_addresses, expected);
        assert_eq!(client_state.min_required_sigs, 2);
        assert_eq!(client_state.latest_height, 50);
    }

    #[tokio::test]
    async fn test_direct_call_by_non_admin_rejected() {
        let admin = Keypair::new();
        let non_admin = Keypair::new();
        let pt = setup_program_test_with_client_state(&admin.pubkey(), &[]);
        let (banks_client, payer, recent_blockhash) = pt.start().await;

        let ix = build_update_attestors_ix(non_admin.pubkey(), vec![[10u8; 20]], 1);

        let tx = solana_sdk::transaction::Transaction::new_signed_with_payer(
            &[ix],
            Some(&payer.pubkey()),
            &[&payer, &non_admin],
            recent_blockhash,
        );
        let err = banks_client.process_transaction(tx).await.unwrap_err();
        assert_eq!(
            extract_custom_error(&err),
            Some(ANCHOR_ERROR_OFFSET + access_manager::AccessManagerError::Unauthorized as u32),
        );
    }

    #[tokio::test]
    async fn test_whitelisted_cpi_succeeds() {
        let admin = Keypair::new();
        let pt = setup_program_test_with_client_state(&admin.pubkey(), &[TEST_CPI_TARGET_ID]);
        let (banks_client, payer, recent_blockhash) = pt.start().await;

        let inner_ix = build_update_attestors_ix(admin.pubkey(), vec![[10u8; 20]], 1);
        let wrapped_ix = wrap_in_test_cpi_target_proxy(admin.pubkey(), &inner_ix);

        let tx = solana_sdk::transaction::Transaction::new_signed_with_payer(
            &[wrapped_ix],
            Some(&payer.pubkey()),
            &[&payer, &admin],
            recent_blockhash,
        );
        let result = banks_client.process_transaction(tx).await;
        assert!(
            result.is_ok(),
            "Whitelisted CPI should succeed: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_unauthorized_cpi_rejected() {
        let admin = Keypair::new();
        let pt = setup_program_test_with_client_state(&admin.pubkey(), &[TEST_CPI_TARGET_ID]);
        let (banks_client, payer, recent_blockhash) = pt.start().await;

        let inner_ix = build_update_attestors_ix(admin.pubkey(), vec![[10u8; 20]], 1);
        let wrapped_ix = wrap_in_test_cpi_proxy(admin.pubkey(), &inner_ix);

        let tx = solana_sdk::transaction::Transaction::new_signed_with_payer(
            &[wrapped_ix],
            Some(&payer.pubkey()),
            &[&payer, &admin],
            recent_blockhash,
        );
        let err = banks_client.process_transaction(tx).await.unwrap_err();
        assert_eq!(
            extract_custom_error(&err),
            Some(ANCHOR_ERROR_OFFSET + access_manager::AccessManagerError::CpiNotAllowed as u32),
        );
    }

    #[tokio::test]
    async fn test_nested_cpi_rejected() {
        let admin = Keypair::new();
        let pt = setup_program_test_with_client_state(&admin.pubkey(), &[TEST_CPI_TARGET_ID]);
        let (banks_client, payer, recent_blockhash) = pt.start().await;

        let inner_ix = build_update_attestors_ix(admin.pubkey(), vec![[10u8; 20]], 1);
        let cpi_target_ix = wrap_in_test_cpi_target_proxy(admin.pubkey(), &inner_ix);
        let nested_ix = wrap_in_test_cpi_proxy(admin.pubkey(), &cpi_target_ix);

        let tx = solana_sdk::transaction::Transaction::new_signed_with_payer(
            &[nested_ix],
            Some(&payer.pubkey()),
            &[&payer, &admin],
            recent_blockhash,
        );
        let err = banks_client.process_transaction(tx).await.unwrap_err();
        assert_eq!(
            extract_custom_error(&err),
            Some(ANCHOR_ERROR_OFFSET + access_manager::AccessManagerError::CpiNotAllowed as u32),
        );
    }
}
