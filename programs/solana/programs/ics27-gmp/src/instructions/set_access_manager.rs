use crate::events::AccessManagerUpdated;
use crate::state::GMPAppState;
use anchor_lang::prelude::*;

#[derive(Accounts)]
#[instruction(new_access_manager: Pubkey)]
pub struct SetAccessManager<'info> {
    #[account(
        mut,
        seeds = [GMPAppState::SEED],
        bump = app_state.bump
    )]
    pub app_state: Account<'info, GMPAppState>,

    /// CHECK: Validated via seeds constraint using the stored `access_manager` program ID
    #[account(
        seeds = [access_manager::state::AccessManager::SEED],
        bump,
        seeds::program = app_state.access_manager
    )]
    pub access_manager: AccountInfo<'info>,

    pub admin: Signer<'info>,

    /// Instructions sysvar for CPI validation
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
    use crate::test_utils::*;
    use access_manager::AccessManagerError;
    use mollusk_svm::result::Check;
    use solana_ibc_types::roles;
    use solana_sdk::instruction::AccountMeta;

    #[test]
    fn test_set_access_manager_success() {
        let admin = Pubkey::new_unique();
        let new_access_manager = Pubkey::new_unique();

        let (app_state_pda, app_state_account) = create_initialized_app_state(access_manager::ID);

        let (access_manager_pda, access_manager_account) =
            create_access_manager_with_role(admin, roles::ADMIN_ROLE, admin);

        let instruction = build_instruction(
            crate::instruction::SetAccessManager { new_access_manager },
            vec![
                AccountMeta::new(app_state_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new_readonly(admin, true),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
        );

        let accounts = vec![
            (app_state_pda, app_state_account),
            (access_manager_pda, access_manager_account),
            (admin, create_signer_account()),
            create_instructions_sysvar_account_with_caller(crate::ID),
        ];

        let mollusk = setup_mollusk();
        let checks = vec![Check::success()];
        let result = mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);

        let app_state = get_app_state_from_result(&result, &app_state_pda);
        assert_eq!(app_state.access_manager, new_access_manager);
    }

    #[test]
    fn test_set_access_manager_not_admin() {
        let admin = Pubkey::new_unique();
        let non_admin = Pubkey::new_unique();
        let new_access_manager = Pubkey::new_unique();

        let (app_state_pda, app_state_account) = create_initialized_app_state(access_manager::ID);

        let (access_manager_pda, access_manager_account) =
            create_access_manager_with_role(admin, roles::ADMIN_ROLE, admin);

        let instruction = build_instruction(
            crate::instruction::SetAccessManager { new_access_manager },
            vec![
                AccountMeta::new(app_state_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new_readonly(non_admin, true),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
        );

        let accounts = vec![
            (app_state_pda, app_state_account),
            (access_manager_pda, access_manager_account),
            (non_admin, create_signer_account()),
            create_instructions_sysvar_account_with_caller(crate::ID),
        ];

        let mollusk = setup_mollusk();
        let checks = vec![Check::err(solana_sdk::program_error::ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + AccessManagerError::Unauthorized as u32,
        ))];

        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }

    #[test]
    fn test_set_access_manager_fake_sysvar_wormhole_attack() {
        let admin = Pubkey::new_unique();
        let new_access_manager = Pubkey::new_unique();

        let (app_state_pda, app_state_account) = create_initialized_app_state(access_manager::ID);

        let (access_manager_pda, access_manager_account) =
            create_access_manager_with_role(admin, roles::ADMIN_ROLE, admin);

        let instruction = build_instruction(
            crate::instruction::SetAccessManager { new_access_manager },
            vec![
                AccountMeta::new(app_state_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new_readonly(admin, true),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
        );

        // Replace real sysvar with fake one (Wormhole-style attack)
        let (instruction, fake_sysvar_account) = setup_fake_sysvar_attack(instruction, crate::ID);

        let accounts = vec![
            (app_state_pda, app_state_account),
            (access_manager_pda, access_manager_account),
            (admin, create_signer_account()),
            fake_sysvar_account,
        ];

        let mollusk = setup_mollusk();
        mollusk.process_and_validate_instruction(
            &instruction,
            &accounts,
            &[expect_sysvar_attack_error()],
        );
    }

    #[test]
    fn test_set_access_manager_cpi_rejection() {
        let admin = Pubkey::new_unique();
        let new_access_manager = Pubkey::new_unique();

        let (app_state_pda, app_state_account) = create_initialized_app_state(access_manager::ID);

        let (access_manager_pda, access_manager_account) =
            create_access_manager_with_role(admin, roles::ADMIN_ROLE, admin);

        let instruction = build_instruction(
            crate::instruction::SetAccessManager { new_access_manager },
            vec![
                AccountMeta::new(app_state_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new_readonly(admin, true),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
        );

        // Simulate CPI call from unauthorized program
        let malicious_program = Pubkey::new_unique();
        let (instruction, cpi_sysvar_account) = setup_cpi_call_test(instruction, malicious_program);

        let accounts = vec![
            (app_state_pda, app_state_account),
            (access_manager_pda, access_manager_account),
            (admin, create_signer_account()),
            cpi_sysvar_account,
        ];

        let mollusk = setup_mollusk();
        let checks = vec![Check::err(solana_sdk::program_error::ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + AccessManagerError::CpiNotAllowed as u32,
        ))];
        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }
}

#[cfg(test)]
mod integration_tests {
    use crate::test_utils::*;
    use anchor_lang::InstructionData;
    use solana_sdk::{
        instruction::{AccountMeta, Instruction},
        pubkey::Pubkey,
        signature::Keypair,
        signer::Signer,
    };

    fn build_set_access_manager_ix(admin: Pubkey) -> Instruction {
        let (app_state_pda, _) =
            Pubkey::find_program_address(&[crate::state::GMPAppState::SEED], &crate::ID);
        let (access_manager_pda, _) =
            solana_ibc_types::access_manager::AccessManager::pda(access_manager::ID);

        let new_access_manager = Pubkey::new_unique();

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
        let pt = setup_program_test_with_access_manager(&admin.pubkey(), &[TEST_CPI_TARGET_ID]);
        let (banks_client, payer, recent_blockhash) = pt.start().await;

        let ix = build_set_access_manager_ix(admin.pubkey());

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
        let pt = setup_program_test_with_access_manager(&admin.pubkey(), &[]);
        let (banks_client, payer, recent_blockhash) = pt.start().await;

        let ix = build_set_access_manager_ix(non_admin.pubkey());

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
            "Non-admin direct call should fail with Unauthorized"
        );
    }

    #[tokio::test]
    async fn test_whitelisted_cpi_succeeds() {
        let admin = Keypair::new();
        let pt = setup_program_test_with_access_manager(&admin.pubkey(), &[TEST_CPI_TARGET_ID]);
        let (banks_client, payer, recent_blockhash) = pt.start().await;

        let inner_ix = build_set_access_manager_ix(admin.pubkey());
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
        let pt = setup_program_test_with_access_manager(&admin.pubkey(), &[TEST_CPI_TARGET_ID]);
        let (banks_client, payer, recent_blockhash) = pt.start().await;

        let inner_ix = build_set_access_manager_ix(admin.pubkey());
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
            "Unauthorized CPI should fail with CpiNotAllowed"
        );
    }

    #[tokio::test]
    async fn test_nested_cpi_rejected() {
        let admin = Keypair::new();
        let pt = setup_program_test_with_access_manager(&admin.pubkey(), &[TEST_CPI_TARGET_ID]);
        let (banks_client, payer, recent_blockhash) = pt.start().await;

        let inner_ix = build_set_access_manager_ix(admin.pubkey());
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
            "Nested CPI should fail with CpiNotAllowed"
        );
    }
}
