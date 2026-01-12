use crate::state::RouterState;
use crate::AccessManagerUpdated;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct SetAccessManager<'info> {
    #[account(
        mut,
        seeds = [RouterState::SEED],
        bump
    )]
    pub router_state: Account<'info, RouterState>,

    /// CHECK: Validated via seeds constraint using the stored `access_manager` program ID
    #[account(
        seeds = [access_manager::state::AccessManager::SEED],
        bump,
        seeds::program = router_state.access_manager
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
    // Performs: CPI rejection + signer verification + role check
    access_manager::require_role(
        &ctx.accounts.access_manager,
        solana_ibc_types::roles::ADMIN_ROLE,
        &ctx.accounts.admin,
        &ctx.accounts.instructions_sysvar,
        &crate::ID,
    )?;

    let old_access_manager = ctx.accounts.router_state.access_manager;

    ctx.accounts.router_state.access_manager = new_access_manager;

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

        let (router_state_pda, router_state_account) = create_initialized_router_state();

        let (access_manager_pda, access_manager_account) =
            create_access_manager_with_role(admin, roles::ADMIN_ROLE, admin);

        let instruction = build_instruction(
            crate::instruction::SetAccessManager { new_access_manager },
            vec![
                AccountMeta::new(router_state_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new_readonly(admin, true),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
        );

        let accounts = vec![
            (router_state_pda, router_state_account),
            (access_manager_pda, access_manager_account),
            (admin, create_signer_account()),
            create_instructions_sysvar_account_with_caller(crate::ID),
        ];

        let mollusk = setup_mollusk();
        let checks = vec![Check::success()];
        let result = mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);

        let router_state = get_router_state_from_result(&result, &router_state_pda);
        assert_eq!(router_state.access_manager, new_access_manager);
    }

    #[test]
    fn test_set_access_manager_not_admin() {
        let admin = Pubkey::new_unique();
        let non_admin = Pubkey::new_unique();
        let new_access_manager = Pubkey::new_unique();

        let (router_state_pda, router_state_account) = create_initialized_router_state();

        let (access_manager_pda, access_manager_account) =
            create_access_manager_with_role(admin, roles::ADMIN_ROLE, admin);

        let instruction = build_instruction(
            crate::instruction::SetAccessManager { new_access_manager },
            vec![
                AccountMeta::new(router_state_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new_readonly(non_admin, true),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
        );

        let accounts = vec![
            (router_state_pda, router_state_account),
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

        let (router_state_pda, router_state_account) = create_initialized_router_state();

        let (access_manager_pda, access_manager_account) =
            create_access_manager_with_role(admin, roles::ADMIN_ROLE, admin);

        let instruction = build_instruction(
            crate::instruction::SetAccessManager { new_access_manager },
            vec![
                AccountMeta::new(router_state_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new_readonly(admin, true),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
        );

        // Replace real sysvar with fake one (Wormhole-style attack)
        let (instruction, fake_sysvar_account) = setup_fake_sysvar_attack(instruction, crate::ID);

        let accounts = vec![
            (router_state_pda, router_state_account),
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

        let (router_state_pda, router_state_account) = create_initialized_router_state();

        let (access_manager_pda, access_manager_account) =
            create_access_manager_with_role(admin, roles::ADMIN_ROLE, admin);

        let instruction = build_instruction(
            crate::instruction::SetAccessManager { new_access_manager },
            vec![
                AccountMeta::new(router_state_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new_readonly(admin, true),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
        );

        // Simulate CPI call from unauthorized program
        let malicious_program = Pubkey::new_unique();
        let (instruction, cpi_sysvar_account) = setup_cpi_call_test(instruction, malicious_program);

        let accounts = vec![
            (router_state_pda, router_state_account),
            (access_manager_pda, access_manager_account),
            (admin, create_signer_account()),
            cpi_sysvar_account,
        ];

        let mollusk = setup_mollusk();

        // When CPI is detected by access_manager::require_role, it returns AccessManagerError::CpiNotAllowed (6005)
        let checks = vec![Check::err(solana_sdk::program_error::ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + access_manager::AccessManagerError::CpiNotAllowed as u32,
        ))];
        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }
}
