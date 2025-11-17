use crate::errors::AccessManagerError;
use crate::events::RoleRevokedEvent;
use crate::state::AccessManager;
use anchor_lang::prelude::*;
use solana_ibc_types::{reject_cpi, roles};

#[derive(Accounts)]
pub struct RenounceRole<'info> {
    #[account(
        mut,
        seeds = [AccessManager::SEED],
        bump
    )]
    pub access_manager: Account<'info, AccessManager>,

    pub caller: Signer<'info>,

    /// Instructions sysvar for CPI validation
    /// CHECK: Address constraint verifies this is the instructions sysvar
    #[account(address = anchor_lang::solana_program::sysvar::instructions::ID)]
    pub instructions_sysvar: AccountInfo<'info>,
}

pub fn renounce_role(ctx: Context<RenounceRole>, role_id: u64) -> Result<()> {
    // Reject CPI calls - this instruction must be called directly
    reject_cpi(&ctx.accounts.instructions_sysvar, &crate::ID).map_err(AccessManagerError::from)?;

    // Cannot renounce PUBLIC_ROLE
    require!(
        role_id != roles::PUBLIC_ROLE,
        AccessManagerError::InvalidRoleId
    );

    let caller_key = ctx.accounts.caller.key();

    // Verify caller has the role they're trying to renounce
    require!(
        ctx.accounts.access_manager.has_role(role_id, &caller_key),
        AccessManagerError::Unauthorized
    );

    // Revoke the role from caller (will fail if trying to remove last admin)
    ctx.accounts
        .access_manager
        .revoke_role(role_id, &caller_key)?;

    emit!(RoleRevokedEvent {
        role_id,
        account: caller_key,
        revoked_by: caller_key, // Self-revocation
    });

    msg!("Role {} renounced by {}", role_id, caller_key);

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
    fn test_renounce_role_success() {
        let relayer = Pubkey::new_unique();
        let admin = Pubkey::new_unique();

        let (access_manager_pda, access_manager_account) =
            create_access_manager_with_role(admin, roles::RELAYER_ROLE, relayer);

        let instruction = build_instruction(
            crate::instruction::RenounceRole {
                role_id: roles::RELAYER_ROLE,
            },
            vec![
                AccountMeta::new(access_manager_pda, false),
                AccountMeta::new_readonly(relayer, true),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
        );

        let accounts = vec![
            (access_manager_pda, access_manager_account),
            (relayer, create_signer_account()),
            (
                solana_sdk::sysvar::instructions::ID,
                create_instructions_sysvar_account(),
            ),
        ];

        let mollusk = setup_mollusk();
        let checks = vec![Check::success()];
        let result = mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);

        let access_manager = get_access_manager_from_result(&result, &access_manager_pda);
        assert!(!access_manager.has_role(roles::RELAYER_ROLE, &relayer));
    }

    #[test]
    fn test_renounce_role_without_having_role() {
        let relayer = Pubkey::new_unique();
        let admin = Pubkey::new_unique();

        let (access_manager_pda, access_manager_account) = create_initialized_access_manager(admin);

        let instruction = build_instruction(
            crate::instruction::RenounceRole {
                role_id: roles::RELAYER_ROLE,
            },
            vec![
                AccountMeta::new(access_manager_pda, false),
                AccountMeta::new_readonly(relayer, true),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
        );

        let accounts = vec![
            (access_manager_pda, access_manager_account),
            (relayer, create_signer_account()),
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
    fn test_renounce_role_cannot_remove_last_admin() {
        let admin = Pubkey::new_unique();

        let (access_manager_pda, access_manager_account) = create_initialized_access_manager(admin);

        let instruction = build_instruction(
            crate::instruction::RenounceRole {
                role_id: roles::ADMIN_ROLE,
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
            ANCHOR_ERROR_OFFSET + AccessManagerError::CannotRemoveLastAdmin as u32,
        ))];

        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }

    #[test]
    fn test_renounce_role_fake_sysvar_wormhole_attack() {
        let admin = Pubkey::new_unique();
        let relayer = Pubkey::new_unique();
        let (access_manager_pda, access_manager_account) =
            create_access_manager_with_role(admin, roles::RELAYER_ROLE, relayer);

        let instruction = build_instruction(
            crate::instruction::RenounceRole {
                role_id: roles::RELAYER_ROLE,
            },
            vec![
                AccountMeta::new(access_manager_pda, false),
                AccountMeta::new_readonly(relayer, true),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
        );

        // Replace real sysvar with fake one (Wormhole-style attack)
        let (instruction, fake_sysvar_account) = setup_fake_sysvar_attack(instruction, crate::ID);

        let accounts = vec![
            (access_manager_pda, access_manager_account),
            (relayer, create_signer_account()),
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
    fn test_renounce_role_cpi_rejection() {
        let relayer = Pubkey::new_unique();
        let role_id = 100;

        let (access_manager_pda, access_manager_account) =
            create_access_manager_with_role(relayer, role_id, relayer);

        let instruction = build_instruction(
            crate::instruction::RenounceRole { role_id },
            vec![
                AccountMeta::new(access_manager_pda, false),
                AccountMeta::new_readonly(relayer, true),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
        );

        // Simulate CPI call from unauthorized program
        let malicious_program = Pubkey::new_unique();
        let (instruction, cpi_sysvar_account) = setup_cpi_call_test(instruction, malicious_program);

        let accounts = vec![
            (access_manager_pda, access_manager_account),
            (relayer, create_signer_account()),
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
