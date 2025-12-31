use crate::events::RoleGranted;
use crate::state::AccessManager;
use crate::{errors::AccessManagerError, WHITELISTED_CPI_PROGRAMS};
use anchor_lang::prelude::*;
use solana_ibc_types::{require_direct_call_or_whitelisted_caller, roles};

#[derive(Accounts)]
pub struct GrantRole<'info> {
    #[account(
        mut,
        seeds = [AccessManager::SEED],
        bump
    )]
    pub access_manager: Account<'info, AccessManager>,

    pub admin: Signer<'info>,

    /// Instructions sysvar for CPI validation
    /// CHECK: Address constraint verifies this is the instructions sysvar
    #[account(address = anchor_lang::solana_program::sysvar::instructions::ID)]
    pub instructions_sysvar: AccountInfo<'info>,
}

pub fn grant_role(ctx: Context<GrantRole>, role_id: u64, account: Pubkey) -> Result<()> {
    // Validate caller
    require_direct_call_or_whitelisted_caller(
        &ctx.accounts.instructions_sysvar,
        WHITELISTED_CPI_PROGRAMS,
        &crate::ID,
    )
    .map_err(AccessManagerError::from)?;

    // Only admins can grant roles
    require!(
        ctx.accounts
            .access_manager
            .has_role(roles::ADMIN_ROLE, &ctx.accounts.admin.key()),
        AccessManagerError::Unauthorized
    );

    // Cannot grant PUBLIC_ROLE
    require!(
        role_id != roles::PUBLIC_ROLE,
        AccessManagerError::InvalidRoleId
    );

    ctx.accounts.access_manager.grant_role(role_id, account)?;

    emit!(RoleGranted {
        role_id,
        account,
        granted_by: ctx.accounts.admin.key(),
    });

    msg!(
        "Role {} granted to {} by {}",
        role_id,
        account,
        ctx.accounts.admin.key()
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::AccessManagerError;
    use crate::test_utils::*;
    use mollusk_svm::result::Check;
    use solana_sdk::instruction::AccountMeta;

    #[test]
    fn test_grant_role_success() {
        let admin = Pubkey::new_unique();
        let relayer = Pubkey::new_unique();

        let (access_manager_pda, access_manager_account) = create_initialized_access_manager(admin);

        let instruction = build_instruction(
            crate::instruction::GrantRole {
                role_id: roles::RELAYER_ROLE,
                account: relayer,
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
        assert!(access_manager.has_role(roles::RELAYER_ROLE, &relayer));
    }

    #[test]
    fn test_grant_role_not_admin() {
        let admin = Pubkey::new_unique();
        let non_admin = Pubkey::new_unique();
        let relayer = Pubkey::new_unique();

        let (access_manager_pda, access_manager_account) = create_initialized_access_manager(admin);

        let instruction = build_instruction(
            crate::instruction::GrantRole {
                role_id: roles::RELAYER_ROLE,
                account: relayer,
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
    fn test_grant_role_invalid_role() {
        let admin = Pubkey::new_unique();
        let account = Pubkey::new_unique();

        let (access_manager_pda, access_manager_account) = create_initialized_access_manager(admin);

        let instruction = build_instruction(
            crate::instruction::GrantRole {
                role_id: roles::PUBLIC_ROLE, // Cannot grant PUBLIC_ROLE
                account,
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
        let checks = vec![Check::err(solana_sdk::program_error::ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + AccessManagerError::InvalidRoleId as u32,
        ))];

        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }

    #[test]
    fn test_grant_role_fake_sysvar_wormhole_attack() {
        let admin = Pubkey::new_unique();
        let relayer = Pubkey::new_unique();

        let (access_manager_pda, access_manager_account) = create_initialized_access_manager(admin);

        let instruction = build_instruction(
            crate::instruction::GrantRole {
                role_id: roles::RELAYER_ROLE,
                account: relayer,
            },
            vec![
                AccountMeta::new(access_manager_pda, false),
                AccountMeta::new_readonly(admin, true),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
        );

        // Replace real sysvar with fake one (Wormhole-style attack)
        let (instruction, fake_sysvar_account) = setup_fake_sysvar_attack(instruction, crate::ID);

        let accounts = vec![
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
    fn test_grant_role_cpi_rejection() {
        let admin = Pubkey::new_unique();
        let member = Pubkey::new_unique();
        let role_id = 100;

        let (access_manager_pda, access_manager_account) =
            create_access_manager_with_role(admin, roles::ADMIN_ROLE, admin);

        let instruction = build_instruction(
            crate::instruction::GrantRole {
                role_id,
                account: member,
            },
            vec![
                AccountMeta::new(access_manager_pda, false),
                AccountMeta::new_readonly(admin, true),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
        );

        // Simulate CPI call from unauthorized program
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
