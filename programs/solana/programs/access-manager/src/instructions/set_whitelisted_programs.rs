use crate::errors::AccessManagerError;
use crate::events::WhitelistedProgramsUpdatedEvent;
use crate::state::AccessManager;
use anchor_lang::prelude::*;
use solana_ibc_types::{require_direct_call_or_whitelisted_caller, roles};

#[derive(Accounts)]
pub struct SetWhitelistedPrograms<'info> {
    #[account(
        mut,
        seeds = [AccessManager::SEED],
        bump
    )]
    pub access_manager: Account<'info, AccessManager>,

    pub admin: Signer<'info>,

    /// CHECK: Address constraint verifies this is the instructions sysvar
    #[account(address = anchor_lang::solana_program::sysvar::instructions::ID)]
    pub instructions_sysvar: AccountInfo<'info>,
}

pub fn set_whitelisted_programs(
    ctx: Context<SetWhitelistedPrograms>,
    whitelisted_programs: Vec<Pubkey>,
) -> Result<()> {
    require_direct_call_or_whitelisted_caller(
        &ctx.accounts.instructions_sysvar,
        &ctx.accounts.access_manager.whitelisted_programs,
        &crate::ID,
    )
    .map_err(AccessManagerError::from)?;

    require!(
        ctx.accounts
            .access_manager
            .has_role(roles::ADMIN_ROLE, &ctx.accounts.admin.key()),
        AccessManagerError::Unauthorized
    );

    let old = ctx.accounts.access_manager.whitelisted_programs.clone();
    ctx.accounts.access_manager.whitelisted_programs = whitelisted_programs.clone();

    emit!(WhitelistedProgramsUpdatedEvent {
        old_programs: old,
        new_programs: whitelisted_programs,
        updated_by: ctx.accounts.admin.key(),
    });

    msg!(
        "Whitelisted programs updated by {}",
        ctx.accounts.admin.key()
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::*;
    use mollusk_svm::result::Check;
    use solana_sdk::instruction::AccountMeta;

    #[test]
    fn test_set_whitelisted_programs_success() {
        let admin = Pubkey::new_unique();
        let program_to_whitelist = Pubkey::new_unique();

        let (access_manager_pda, access_manager_account) = create_initialized_access_manager(admin);

        let instruction = build_instruction(
            crate::instruction::SetWhitelistedPrograms {
                whitelisted_programs: vec![program_to_whitelist],
            },
            vec![
                AccountMeta::new(access_manager_pda, false),
                AccountMeta::new_readonly(admin, true),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
        );

        let accounts = vec![
            (access_manager_pda, access_manager_account),
            (admin, create_signer_account()),
            (
                solana_sdk::sysvar::instructions::ID,
                create_instructions_sysvar_account(),
            ),
        ];

        let mollusk = setup_mollusk();
        let checks = vec![Check::success()];
        let result = mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);

        let access_manager = get_access_manager_from_result(&result, &access_manager_pda);
        assert_eq!(
            access_manager.whitelisted_programs,
            vec![program_to_whitelist]
        );
    }

    #[test]
    fn test_set_whitelisted_programs_not_admin() {
        let admin = Pubkey::new_unique();
        let non_admin = Pubkey::new_unique();

        let (access_manager_pda, access_manager_account) = create_initialized_access_manager(admin);

        let instruction = build_instruction(
            crate::instruction::SetWhitelistedPrograms {
                whitelisted_programs: vec![Pubkey::new_unique()],
            },
            vec![
                AccountMeta::new(access_manager_pda, false),
                AccountMeta::new_readonly(non_admin, true),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
        );

        let accounts = vec![
            (access_manager_pda, access_manager_account),
            (non_admin, create_signer_account()),
            (
                solana_sdk::sysvar::instructions::ID,
                create_instructions_sysvar_account(),
            ),
        ];

        let mollusk = setup_mollusk();
        let checks = vec![Check::err(solana_sdk::program_error::ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + AccessManagerError::Unauthorized as u32,
        ))];

        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }

    #[test]
    fn test_set_whitelisted_programs_cpi_rejection() {
        let admin = Pubkey::new_unique();

        let (access_manager_pda, access_manager_account) = create_initialized_access_manager(admin);

        let instruction = build_instruction(
            crate::instruction::SetWhitelistedPrograms {
                whitelisted_programs: vec![Pubkey::new_unique()],
            },
            vec![
                AccountMeta::new(access_manager_pda, false),
                AccountMeta::new_readonly(admin, true),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
        );

        let malicious_program = Pubkey::new_unique();
        let (instruction, cpi_sysvar_account) = setup_cpi_call_test(instruction, malicious_program);

        let accounts = vec![
            (access_manager_pda, access_manager_account),
            (admin, create_signer_account()),
            cpi_sysvar_account,
        ];

        let mollusk = setup_mollusk();
        mollusk.process_and_validate_instruction(
            &instruction,
            &accounts,
            &[expect_access_manager_cpi_rejection_error()],
        );
    }
}
