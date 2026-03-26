use crate::errors::AccessManagerError;
use crate::events::UpgradeAuthorityTransferredEvent;
use crate::helpers::require_admin;
use crate::state::AccessManager;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::bpf_loader_upgradeable;

/// Transfers a target program's BPF Loader upgrade authority from this access manager's PDA to a
/// new authority. Requires admin authorization.
///
/// This enables access manager migration: the current AM signs the BPF Loader `SetAuthority` call
/// to hand over upgrade control to a new address (keypair or another AM's PDA).
///
/// This operation is irreversible from this access manager's perspective. Once transferred, only
/// the new authority can upgrade the target program or transfer authority again.
#[derive(Accounts)]
#[instruction(target_program: Pubkey, new_authority: Pubkey)]
pub struct TransferUpgradeAuthority<'info> {
    /// The access manager PDA for admin authorization.
    #[account(
        seeds = [AccessManager::SEED],
        bump
    )]
    pub access_manager: Account<'info, AccessManager>,

    /// The target program's data account (BPF Loader Upgradeable PDA).
    /// CHECK: Validated via BPF Loader seeds derivation from target program
    #[account(
        mut,
        seeds = [target_program.as_ref()],
        bump,
        seeds::program = bpf_loader_upgradeable::ID
    )]
    pub program_data: AccountInfo<'info>,

    /// `AccessManager`'s PDA that acts as the current upgrade authority for the target program.
    /// Not mutable because BPF Loader's `SetAuthority` only reads the signer.
    /// CHECK: Validated via seeds constraint
    #[account(
        seeds = [AccessManager::UPGRADE_AUTHORITY_SEED, target_program.as_ref()],
        bump
    )]
    pub upgrade_authority: AccountInfo<'info>,

    /// The new upgrade authority to transfer to.
    /// CHECK: Must match the `new_authority` instruction parameter
    #[account(
        constraint = new_authority_account.key() == new_authority @ AccessManagerError::AuthorityMismatch
    )]
    pub new_authority_account: AccountInfo<'info>,

    /// The admin signer authorizing the transfer.
    pub authority: Signer<'info>,

    /// CHECK: Address constraint verifies this is the instructions sysvar
    #[account(address = anchor_lang::solana_program::sysvar::instructions::ID)]
    pub instructions_sysvar: AccountInfo<'info>,

    /// CHECK: Must be BPF Loader Upgradeable program ID
    #[account(address = bpf_loader_upgradeable::ID)]
    pub bpf_loader_upgradeable: AccountInfo<'info>,
}

pub fn transfer_upgrade_authority(
    ctx: Context<TransferUpgradeAuthority>,
    target_program: Pubkey,
    new_authority: Pubkey,
) -> Result<()> {
    require_admin(
        &ctx.accounts.access_manager.to_account_info(),
        &ctx.accounts.authority.to_account_info(),
        &ctx.accounts.instructions_sysvar,
        &crate::ID,
    )?;

    require!(
        new_authority != Pubkey::default(),
        AccessManagerError::ZeroAccount
    );

    let (upgrade_authority_pda, bump) =
        AccessManager::upgrade_authority_pda(&target_program, &crate::ID);

    let set_authority_ix = bpf_loader_upgradeable::set_upgrade_authority(
        &target_program,
        &upgrade_authority_pda,
        Some(&new_authority),
    );

    // BPF Loader SetAuthority expects:
    //   [0] programdata_address  (writable, non-signer)
    //   [1] current_authority    (read-only, signer via PDA)
    //   [2] new_authority        (read-only, non-signer)
    anchor_lang::solana_program::program::invoke_signed(
        &set_authority_ix,
        &[
            ctx.accounts.program_data.to_account_info(),
            ctx.accounts.upgrade_authority.to_account_info(),
            ctx.accounts.new_authority_account.to_account_info(),
        ],
        &[&[
            AccessManager::UPGRADE_AUTHORITY_SEED,
            target_program.as_ref(),
            &[bump],
        ]],
    )?;

    emit!(UpgradeAuthorityTransferredEvent {
        program: target_program,
        old_authority: upgrade_authority_pda,
        new_authority,
        transferred_by: ctx.accounts.authority.key(),
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::*;
    use mollusk_svm::result::Check;
    use solana_sdk::{account::Account, instruction::AccountMeta};

    fn derive_program_data(target_program: &Pubkey) -> Pubkey {
        Pubkey::find_program_address(&[target_program.as_ref()], &bpf_loader_upgradeable::ID).0
    }

    fn build_transfer_account_metas(
        access_manager_pda: Pubkey,
        program_data_address: Pubkey,
        upgrade_authority_pda: Pubkey,
        new_authority: Pubkey,
        authority: Pubkey,
    ) -> Vec<AccountMeta> {
        vec![
            AccountMeta::new_readonly(access_manager_pda, false),
            AccountMeta::new(program_data_address, false),
            AccountMeta::new_readonly(upgrade_authority_pda, false),
            AccountMeta::new_readonly(new_authority, false),
            AccountMeta::new_readonly(authority, true),
            AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            AccountMeta::new_readonly(bpf_loader_upgradeable::ID, false),
        ]
    }

    fn create_transfer_accounts(
        program_data_address: Pubkey,
        upgrade_authority_pda: Pubkey,
        new_authority: Pubkey,
        authority: Pubkey,
    ) -> Vec<(Pubkey, Account)> {
        vec![
            (
                program_data_address,
                Account {
                    lamports: 1_000_000,
                    data: vec![0; 100],
                    owner: bpf_loader_upgradeable::ID,
                    ..Default::default()
                },
            ),
            (
                upgrade_authority_pda,
                Account {
                    lamports: 1_000_000,
                    owner: crate::ID,
                    ..Default::default()
                },
            ),
            (
                new_authority,
                Account {
                    lamports: 1_000_000,
                    owner: solana_sdk::system_program::ID,
                    ..Default::default()
                },
            ),
            (authority, create_signer_account()),
            (
                bpf_loader_upgradeable::ID,
                Account {
                    lamports: 1_000_000,
                    executable: true,
                    owner: solana_sdk::native_loader::ID,
                    ..Default::default()
                },
            ),
        ]
    }

    fn setup_transfer_test(
        admin: Pubkey,
        target_program: Pubkey,
        new_authority: Pubkey,
    ) -> (Pubkey, Account, Pubkey, Pubkey, Vec<AccountMeta>) {
        let (access_manager_pda, access_manager_account) = create_initialized_access_manager(admin);
        let (upgrade_authority_pda, _) =
            AccessManager::upgrade_authority_pda(&target_program, &crate::ID);
        let program_data_address = derive_program_data(&target_program);

        let account_metas = build_transfer_account_metas(
            access_manager_pda,
            program_data_address,
            upgrade_authority_pda,
            new_authority,
            admin,
        );

        (
            access_manager_pda,
            access_manager_account,
            upgrade_authority_pda,
            program_data_address,
            account_metas,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn build_transfer_instruction_and_accounts(
        access_manager_pda: Pubkey,
        access_manager_account: Account,
        target_program: Pubkey,
        program_data_address: Pubkey,
        upgrade_authority_pda: Pubkey,
        new_authority: Pubkey,
        authority: Pubkey,
        sysvar_account: (Pubkey, Account),
    ) -> (solana_sdk::instruction::Instruction, Vec<(Pubkey, Account)>) {
        let account_metas = build_transfer_account_metas(
            access_manager_pda,
            program_data_address,
            upgrade_authority_pda,
            new_authority,
            authority,
        );

        let instruction = build_instruction(
            crate::instruction::TransferUpgradeAuthority {
                target_program,
                new_authority,
            },
            account_metas,
        );

        let mut accounts = vec![(access_manager_pda, access_manager_account)];
        accounts.extend(create_transfer_accounts(
            program_data_address,
            upgrade_authority_pda,
            new_authority,
            authority,
        ));
        accounts.push(sysvar_account);

        (instruction, accounts)
    }

    #[test]
    #[ignore = "Requires full integration test setup with BPF Loader"]
    fn test_transfer_upgrade_authority_success() {
        let admin = Pubkey::new_unique();
        let target_program = Pubkey::new_unique();
        let new_authority = Pubkey::new_unique();

        let (
            access_manager_pda,
            access_manager_account,
            upgrade_authority_pda,
            program_data_address,
            account_metas,
        ) = setup_transfer_test(admin, target_program, new_authority);

        let instruction = build_instruction(
            crate::instruction::TransferUpgradeAuthority {
                target_program,
                new_authority,
            },
            account_metas,
        );

        let mut accounts = vec![(access_manager_pda, access_manager_account)];
        accounts.extend(create_transfer_accounts(
            program_data_address,
            upgrade_authority_pda,
            new_authority,
            admin,
        ));
        accounts.push(create_instructions_sysvar_account_with_caller(crate::ID));

        let mollusk = setup_mollusk();
        mollusk.process_and_validate_instruction(&instruction, &accounts, &[Check::success()]);
    }

    #[test]
    fn test_transfer_upgrade_authority_not_admin() {
        let admin = Pubkey::new_unique();
        let non_admin = Pubkey::new_unique();
        let target_program = Pubkey::new_unique();
        let new_authority = Pubkey::new_unique();

        let (access_manager_pda, access_manager_account) = create_initialized_access_manager(admin);
        let (upgrade_authority_pda, _) =
            AccessManager::upgrade_authority_pda(&target_program, &crate::ID);
        let program_data_address = derive_program_data(&target_program);

        let (instruction, accounts) = build_transfer_instruction_and_accounts(
            access_manager_pda,
            access_manager_account,
            target_program,
            program_data_address,
            upgrade_authority_pda,
            new_authority,
            non_admin,
            create_instructions_sysvar_account_with_caller(crate::ID),
        );

        let mollusk = setup_mollusk();
        mollusk.process_and_validate_instruction(
            &instruction,
            &accounts,
            &[Check::err(solana_sdk::program_error::ProgramError::Custom(
                ANCHOR_ERROR_OFFSET + AccessManagerError::Unauthorized as u32,
            ))],
        );
    }

    #[test]
    fn test_transfer_upgrade_authority_cpi_rejection() {
        let admin = Pubkey::new_unique();
        let target_program = Pubkey::new_unique();
        let new_authority = Pubkey::new_unique();

        let (
            access_manager_pda,
            access_manager_account,
            upgrade_authority_pda,
            program_data_address,
            account_metas,
        ) = setup_transfer_test(admin, target_program, new_authority);

        let instruction = build_instruction(
            crate::instruction::TransferUpgradeAuthority {
                target_program,
                new_authority,
            },
            account_metas,
        );

        let malicious_program = Pubkey::new_unique();
        let (instruction, cpi_sysvar_account) = setup_cpi_call_test(instruction, malicious_program);

        let mut accounts = vec![(access_manager_pda, access_manager_account)];
        accounts.extend(create_transfer_accounts(
            program_data_address,
            upgrade_authority_pda,
            new_authority,
            admin,
        ));
        accounts.push(cpi_sysvar_account);

        let mollusk = setup_mollusk();
        mollusk.process_and_validate_instruction(
            &instruction,
            &accounts,
            &[expect_access_manager_cpi_rejection_error()],
        );
    }

    #[test]
    fn test_transfer_upgrade_authority_fake_sysvar_wormhole_attack() {
        let admin = Pubkey::new_unique();
        let target_program = Pubkey::new_unique();
        let new_authority = Pubkey::new_unique();

        let (
            access_manager_pda,
            access_manager_account,
            upgrade_authority_pda,
            program_data_address,
            account_metas,
        ) = setup_transfer_test(admin, target_program, new_authority);

        let instruction = build_instruction(
            crate::instruction::TransferUpgradeAuthority {
                target_program,
                new_authority,
            },
            account_metas,
        );

        let (instruction, fake_sysvar_account) = setup_fake_sysvar_attack(instruction, crate::ID);

        let mut accounts = vec![(access_manager_pda, access_manager_account)];
        accounts.extend(create_transfer_accounts(
            program_data_address,
            upgrade_authority_pda,
            new_authority,
            admin,
        ));
        accounts.push(fake_sysvar_account);

        let mollusk = setup_mollusk();
        mollusk.process_and_validate_instruction(
            &instruction,
            &accounts,
            &[expect_sysvar_attack_error()],
        );
    }

    #[test]
    fn test_transfer_upgrade_authority_wrong_pda() {
        let admin = Pubkey::new_unique();
        let target_program = Pubkey::new_unique();
        let new_authority = Pubkey::new_unique();

        let (access_manager_pda, access_manager_account) = create_initialized_access_manager(admin);
        let wrong_upgrade_authority = Pubkey::new_unique();
        let program_data_address = derive_program_data(&target_program);

        let (instruction, accounts) = build_transfer_instruction_and_accounts(
            access_manager_pda,
            access_manager_account,
            target_program,
            program_data_address,
            wrong_upgrade_authority,
            new_authority,
            admin,
            create_instructions_sysvar_account_with_caller(crate::ID),
        );

        let mollusk = setup_mollusk();
        mollusk.process_and_validate_instruction(
            &instruction,
            &accounts,
            &[Check::err(solana_sdk::program_error::ProgramError::Custom(
                anchor_lang::error::ErrorCode::ConstraintSeeds as u32,
            ))],
        );
    }

    #[test]
    fn test_transfer_upgrade_authority_zero_address() {
        let admin = Pubkey::new_unique();
        let target_program = Pubkey::new_unique();
        let zero_authority = Pubkey::default();

        let (access_manager_pda, access_manager_account) = create_initialized_access_manager(admin);
        let (upgrade_authority_pda, _) =
            AccessManager::upgrade_authority_pda(&target_program, &crate::ID);
        let program_data_address = derive_program_data(&target_program);

        let (instruction, accounts) = build_transfer_instruction_and_accounts(
            access_manager_pda,
            access_manager_account,
            target_program,
            program_data_address,
            upgrade_authority_pda,
            zero_authority,
            admin,
            create_instructions_sysvar_account_with_caller(crate::ID),
        );

        let mollusk = setup_mollusk();
        mollusk.process_and_validate_instruction(
            &instruction,
            &accounts,
            &[Check::err(solana_sdk::program_error::ProgramError::Custom(
                ANCHOR_ERROR_OFFSET + AccessManagerError::ZeroAccount as u32,
            ))],
        );
    }
}

#[cfg(test)]
mod integration_tests {
    use crate::state::AccessManager;
    use crate::test_utils::*;
    use anchor_lang::prelude::bpf_loader_upgradeable;
    use anchor_lang::InstructionData;
    use solana_sdk::{
        account::Account,
        bpf_loader_upgradeable::UpgradeableLoaderState,
        instruction::{AccountMeta, Instruction},
        pubkey::Pubkey,
        signature::Keypair,
        signer::Signer,
    };

    fn setup_transfer_program_test(
        admin: &Pubkey,
        whitelisted: &[Pubkey],
    ) -> (solana_program_test::ProgramTest, Pubkey) {
        let mut pt = setup_program_test_with_whitelist(admin, whitelisted);

        let target_program = Pubkey::new_unique();
        let (program_data_pda, _) =
            Pubkey::find_program_address(&[target_program.as_ref()], &bpf_loader_upgradeable::ID);
        let (upgrade_authority_pda, _) =
            AccessManager::upgrade_authority_pda(&target_program, &crate::ID);
        let new_authority = Pubkey::new_unique();

        // ProgramData with upgrade_authority set to AccessManager's PDA
        let pd_account = Account::new_data_with_space(
            10_000_000_000,
            &UpgradeableLoaderState::ProgramData {
                slot: 0,
                upgrade_authority_address: Some(upgrade_authority_pda),
            },
            UpgradeableLoaderState::size_of_programdata_metadata(),
            &bpf_loader_upgradeable::ID,
        )
        .unwrap();
        pt.add_account(program_data_pda, pd_account);

        // Upgrade authority PDA account
        pt.add_account(
            upgrade_authority_pda,
            Account {
                lamports: 1_000_000,
                owner: solana_sdk::system_program::ID,
                ..Default::default()
            },
        );

        // New authority account
        pt.add_account(
            new_authority,
            Account {
                lamports: 1_000_000,
                owner: solana_sdk::system_program::ID,
                ..Default::default()
            },
        );

        (pt, target_program)
    }

    fn build_transfer_ix(
        authority: Pubkey,
        target_program: Pubkey,
        new_authority: Pubkey,
    ) -> Instruction {
        let (access_manager_pda, _) =
            Pubkey::find_program_address(&[AccessManager::SEED], &crate::ID);
        let (program_data, _) =
            Pubkey::find_program_address(&[target_program.as_ref()], &bpf_loader_upgradeable::ID);
        let (upgrade_authority_pda, _) =
            AccessManager::upgrade_authority_pda(&target_program, &crate::ID);

        Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new(program_data, false),
                AccountMeta::new_readonly(upgrade_authority_pda, false),
                AccountMeta::new_readonly(new_authority, false),
                AccountMeta::new_readonly(authority, true),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
                AccountMeta::new_readonly(bpf_loader_upgradeable::ID, false),
            ],
            data: crate::instruction::TransferUpgradeAuthority {
                target_program,
                new_authority,
            }
            .data(),
        }
    }

    async fn get_program_data_authority(
        banks_client: &solana_program_test::BanksClient,
        target_program: Pubkey,
    ) -> Option<Pubkey> {
        let (program_data_pda, _) =
            Pubkey::find_program_address(&[target_program.as_ref()], &bpf_loader_upgradeable::ID);
        let account = banks_client
            .get_account(program_data_pda)
            .await
            .unwrap()
            .unwrap();
        let state: UpgradeableLoaderState = bincode::deserialize(&account.data).unwrap();
        match state {
            UpgradeableLoaderState::ProgramData {
                upgrade_authority_address,
                ..
            } => upgrade_authority_address,
            _ => panic!("unexpected state"),
        }
    }

    #[tokio::test]
    async fn test_transfer_direct_call_by_admin_succeeds() {
        let admin = Keypair::new();
        let new_authority = Pubkey::new_unique();
        let (pt, target_program) =
            setup_transfer_program_test(&admin.pubkey(), &[TEST_CPI_TARGET_ID]);
        let (banks_client, payer, recent_blockhash) = pt.start().await;

        let ix = build_transfer_ix(admin.pubkey(), target_program, new_authority);

        let tx = solana_sdk::transaction::Transaction::new_signed_with_payer(
            &[ix],
            Some(&payer.pubkey()),
            &[&payer, &admin],
            recent_blockhash,
        );
        let result = banks_client.process_transaction(tx).await;
        assert!(
            result.is_ok(),
            "Direct transfer by admin should succeed: {:?}",
            result.err()
        );

        let authority = get_program_data_authority(&banks_client, target_program).await;
        assert_eq!(
            authority,
            Some(new_authority),
            "upgrade authority should be transferred to new authority"
        );
    }

    #[tokio::test]
    async fn test_transfer_whitelisted_cpi_succeeds() {
        let admin = Keypair::new();
        let new_authority = Pubkey::new_unique();
        let (pt, target_program) =
            setup_transfer_program_test(&admin.pubkey(), &[TEST_CPI_TARGET_ID]);
        let (banks_client, payer, recent_blockhash) = pt.start().await;

        let inner_ix = build_transfer_ix(admin.pubkey(), target_program, new_authority);
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
            "Whitelisted CPI transfer should succeed: {:?}",
            result.err()
        );

        let authority = get_program_data_authority(&banks_client, target_program).await;
        assert_eq!(
            authority,
            Some(new_authority),
            "upgrade authority should be transferred via whitelisted CPI"
        );
    }

    #[tokio::test]
    async fn test_transfer_non_admin_rejected() {
        let admin = Keypair::new();
        let non_admin = Keypair::new();
        let new_authority = Pubkey::new_unique();
        let (pt, target_program) =
            setup_transfer_program_test(&admin.pubkey(), &[TEST_CPI_TARGET_ID]);
        let (banks_client, payer, recent_blockhash) = pt.start().await;

        let ix = build_transfer_ix(non_admin.pubkey(), target_program, new_authority);

        let tx = solana_sdk::transaction::Transaction::new_signed_with_payer(
            &[ix],
            Some(&payer.pubkey()),
            &[&payer, &non_admin],
            recent_blockhash,
        );
        let err = banks_client.process_transaction(tx).await.unwrap_err();
        assert_eq!(
            extract_custom_error(&err),
            Some(ANCHOR_ERROR_OFFSET + crate::AccessManagerError::Unauthorized as u32),
        );
    }

    #[tokio::test]
    async fn test_transfer_unauthorized_cpi_rejected() {
        let admin = Keypair::new();
        let new_authority = Pubkey::new_unique();
        let (pt, target_program) =
            setup_transfer_program_test(&admin.pubkey(), &[TEST_CPI_TARGET_ID]);
        let (banks_client, payer, recent_blockhash) = pt.start().await;

        let inner_ix = build_transfer_ix(admin.pubkey(), target_program, new_authority);
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
            Some(ANCHOR_ERROR_OFFSET + crate::AccessManagerError::CpiNotAllowed as u32),
        );
    }

    #[tokio::test]
    async fn test_transfer_nested_cpi_rejected() {
        let admin = Keypair::new();
        let new_authority = Pubkey::new_unique();
        let (pt, target_program) =
            setup_transfer_program_test(&admin.pubkey(), &[TEST_CPI_TARGET_ID]);
        let (banks_client, payer, recent_blockhash) = pt.start().await;

        let inner_ix = build_transfer_ix(admin.pubkey(), target_program, new_authority);
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
            Some(ANCHOR_ERROR_OFFSET + crate::AccessManagerError::CpiNotAllowed as u32),
        );
    }
}
