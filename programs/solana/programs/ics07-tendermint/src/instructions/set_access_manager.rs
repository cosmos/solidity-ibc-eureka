use crate::events::AccessManagerUpdated;
use crate::types::AppState;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct SetAccessManager<'info> {
    #[account(
        mut,
        seeds = [AppState::SEED],
        bump
    )]
    pub app_state: Account<'info, AppState>,

    /// CHECK: Validated via seeds constraint using the stored `access_manager` program ID
    #[account(
        seeds = [access_manager::state::AccessManager::SEED],
        bump,
        seeds::program = app_state.access_manager
    )]
    pub access_manager: AccountInfo<'info>,

    pub admin: Signer<'info>,

    /// CHECK: Address constraint verifies this is the instructions sysvar
    #[account(address = anchor_lang::solana_program::sysvar::instructions::ID)]
    pub instructions_sysvar: AccountInfo<'info>,
}

pub fn set_access_manager(
    ctx: Context<SetAccessManager>,
    new_access_manager: Pubkey,
) -> Result<()> {
    access_manager::require_admin(
        &ctx.accounts.access_manager,
        &ctx.accounts.admin,
        &ctx.accounts.instructions_sysvar,
        &crate::ID,
    )?;

    let old_access_manager = ctx.accounts.app_state.access_manager;

    ctx.accounts.app_state.access_manager = new_access_manager;

    emit!(AccessManagerUpdated {
        old_access_manager,
        new_access_manager,
    });

    msg!(
        "Access manager updated from {} to {}",
        old_access_manager,
        new_access_manager
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::PROGRAM_BINARY_PATH;
    use crate::types::AppState;
    use access_manager::AccessManagerError;
    use anchor_lang::InstructionData;
    use mollusk_svm::result::Check;
    use mollusk_svm::Mollusk;
    use solana_ibc_types::roles;
    use solana_sdk::account::Account as SolanaAccount;
    use solana_sdk::instruction::{AccountMeta, Instruction};
    use solana_sdk::pubkey::Pubkey;

    const ANCHOR_ERROR_OFFSET: u32 = 6000;

    fn create_signer_account() -> SolanaAccount {
        SolanaAccount {
            lamports: 1_000_000_000,
            data: vec![],
            owner: solana_sdk::system_program::ID,
            executable: false,
            rent_epoch: 0,
        }
    }

    fn create_app_state_account(access_manager: Pubkey) -> SolanaAccount {
        use anchor_lang::AccountSerialize;

        let app_state = AppState {
            access_manager,
            chain_id: String::new(),
            _reserved: [0; 256],
        };

        let mut data = vec![0u8; 8 + AppState::INIT_SPACE];
        app_state.try_serialize(&mut &mut data[..]).unwrap();

        SolanaAccount {
            lamports: 10_000_000,
            data,
            owner: crate::ID,
            executable: false,
            rent_epoch: 0,
        }
    }

    fn create_access_manager_account(admin: Pubkey, role: u64) -> SolanaAccount {
        use access_manager::state::AccessManager;
        use access_manager::types::RoleData;
        use anchor_lang::AccountSerialize;

        let access_manager = AccessManager {
            roles: vec![RoleData {
                role_id: role,
                members: vec![admin],
            }],
            whitelisted_programs: vec![],
        };

        let mut data = vec![0u8; 8 + 10000]; // Enough space
        access_manager.try_serialize(&mut &mut data[..]).unwrap();

        SolanaAccount {
            lamports: 10_000_000,
            data,
            owner: access_manager::ID,
            executable: false,
            rent_epoch: 0,
        }
    }

    fn create_instructions_sysvar_account() -> (Pubkey, SolanaAccount) {
        use solana_sdk::sysvar::instructions::{
            construct_instructions_data, BorrowedAccountMeta, BorrowedInstruction,
        };

        let account_pubkey = Pubkey::new_unique();
        let account = BorrowedAccountMeta {
            pubkey: &account_pubkey,
            is_signer: false,
            is_writable: true,
        };
        let mock_instruction = BorrowedInstruction {
            program_id: &crate::ID,
            accounts: vec![account],
            data: &[],
        };

        let ixs_data = construct_instructions_data(&[mock_instruction]);

        (
            solana_sdk::sysvar::instructions::ID,
            SolanaAccount {
                lamports: 1_000_000,
                data: ixs_data,
                owner: solana_sdk::sysvar::ID,
                executable: false,
                rent_epoch: 0,
            },
        )
    }

    #[test]
    fn test_set_access_manager_success() {
        let admin = Pubkey::new_unique();
        let new_access_manager = Pubkey::new_unique();

        let (app_state_pda, _) = Pubkey::find_program_address(&[AppState::SEED], &crate::ID);

        let (access_manager_pda, _) = Pubkey::find_program_address(
            &[access_manager::state::AccessManager::SEED],
            &access_manager::ID,
        );

        let instruction_data = crate::instruction::SetAccessManager { new_access_manager };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(app_state_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new_readonly(admin, true),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
            data: instruction_data.data(),
        };

        let app_state_account = create_app_state_account(access_manager::ID);
        let access_manager_account = create_access_manager_account(admin, roles::ADMIN_ROLE);
        let (instructions_sysvar_pubkey, instructions_sysvar_account) =
            create_instructions_sysvar_account();

        let accounts = vec![
            (app_state_pda, app_state_account),
            (access_manager_pda, access_manager_account),
            (admin, create_signer_account()),
            (instructions_sysvar_pubkey, instructions_sysvar_account),
        ];

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::success()];
        let result = mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);

        let app_state_account = result
            .get_account(&app_state_pda)
            .expect("App state account not found");
        let app_state: AppState = AppState::try_deserialize(&mut &app_state_account.data[..])
            .expect("Failed to deserialize app state");

        assert_eq!(app_state.access_manager, new_access_manager);
    }

    #[test]
    fn test_set_access_manager_not_admin_fails() {
        let admin = Pubkey::new_unique();
        let non_admin = Pubkey::new_unique();
        let new_access_manager = Pubkey::new_unique();

        let (app_state_pda, _) = Pubkey::find_program_address(&[AppState::SEED], &crate::ID);

        let (access_manager_pda, _) = Pubkey::find_program_address(
            &[access_manager::state::AccessManager::SEED],
            &access_manager::ID,
        );

        let instruction_data = crate::instruction::SetAccessManager { new_access_manager };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(app_state_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new_readonly(non_admin, true),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
            data: instruction_data.data(),
        };

        let app_state_account = create_app_state_account(access_manager::ID);
        let access_manager_account = create_access_manager_account(admin, roles::ADMIN_ROLE);
        let (instructions_sysvar_pubkey, instructions_sysvar_account) =
            create_instructions_sysvar_account();

        let accounts = vec![
            (app_state_pda, app_state_account),
            (access_manager_pda, access_manager_account),
            (non_admin, create_signer_account()),
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
    use anchor_lang::InstructionData;
    use solana_sdk::{
        instruction::{AccountMeta, Instruction},
        pubkey::Pubkey,
        signature::Keypair,
        signer::Signer,
    };

    fn build_set_access_manager_ix(admin: Pubkey, new_access_manager: Pubkey) -> Instruction {
        let (app_state_pda, _) =
            Pubkey::find_program_address(&[crate::types::AppState::SEED], &crate::ID);
        let (access_manager_pda, _) = Pubkey::find_program_address(
            &[access_manager::state::AccessManager::SEED],
            &access_manager::ID,
        );

        Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(app_state_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new_readonly(admin, true),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
            data: crate::instruction::SetAccessManager { new_access_manager }.data(),
        }
    }

    #[tokio::test]
    async fn test_direct_call_by_admin_succeeds() {
        let admin = Keypair::new();
        let pt = setup_program_test_with_whitelist(&admin.pubkey(), &[CPI_TEST_TARGET_ID]);
        let (banks_client, payer, recent_blockhash) = pt.start().await;

        let ix = build_set_access_manager_ix(admin.pubkey(), Pubkey::new_unique());

        let tx = solana_sdk::transaction::Transaction::new_signed_with_payer(
            &[ix],
            Some(&payer.pubkey()),
            &[&payer, &admin],
            recent_blockhash,
        );
        let result = banks_client.process_transaction(tx).await;
        assert!(result.is_ok(), "Direct call by admin should succeed");
    }

    #[tokio::test]
    async fn test_direct_call_by_non_admin_rejected() {
        let admin = Keypair::new();
        let non_admin = Keypair::new();
        let pt = setup_program_test_with_whitelist(&admin.pubkey(), &[]);
        let (banks_client, payer, recent_blockhash) = pt.start().await;

        let ix = build_set_access_manager_ix(non_admin.pubkey(), Pubkey::new_unique());

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
        let pt = setup_program_test_with_whitelist(&admin.pubkey(), &[CPI_TEST_TARGET_ID]);
        let (banks_client, payer, recent_blockhash) = pt.start().await;

        let inner_ix = build_set_access_manager_ix(admin.pubkey(), Pubkey::new_unique());
        let wrapped_ix = wrap_in_cpi_test_target_proxy(admin.pubkey(), &inner_ix);

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
        let pt = setup_program_test_with_whitelist(&admin.pubkey(), &[CPI_TEST_TARGET_ID]);
        let (banks_client, payer, recent_blockhash) = pt.start().await;

        let inner_ix = build_set_access_manager_ix(admin.pubkey(), Pubkey::new_unique());
        let wrapped_ix = wrap_in_proxy_cpi(admin.pubkey(), &inner_ix);

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
        let pt = setup_program_test_with_whitelist(&admin.pubkey(), &[CPI_TEST_TARGET_ID]);
        let (banks_client, payer, recent_blockhash) = pt.start().await;

        let inner_ix = build_set_access_manager_ix(admin.pubkey(), Pubkey::new_unique());
        let cpi_target_ix = wrap_in_cpi_test_target_proxy(admin.pubkey(), &inner_ix);
        let nested_ix = wrap_in_proxy_cpi(admin.pubkey(), &cpi_target_ix);

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
