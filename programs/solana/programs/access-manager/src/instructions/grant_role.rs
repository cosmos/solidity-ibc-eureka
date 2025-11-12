use crate::events::RoleGrantedEvent;
use crate::state::AccessManager;
use crate::types::AccessManagerError;
use anchor_lang::prelude::*;
use solana_ibc_types::roles;

#[derive(Accounts)]
pub struct GrantRole<'info> {
    #[account(
        mut,
        seeds = [AccessManager::SEED],
        bump
    )]
    pub access_manager: Account<'info, AccessManager>,

    #[account(
        constraint = admin.key() == access_manager.admin @ AccessManagerError::NotAdmin
    )]
    pub admin: Signer<'info>,
}

pub fn grant_role(ctx: Context<GrantRole>, role_id: u64, account: Pubkey) -> Result<()> {
    require!(
        role_id != roles::PUBLIC_ROLE && role_id != roles::ADMIN_ROLE,
        AccessManagerError::InvalidRoleId
    );

    ctx.accounts.access_manager.grant_role(role_id, account)?;

    emit!(RoleGrantedEvent {
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
    use crate::test_utils::*;
    use crate::types::AccessManagerError;
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
            ],
        );

        let accounts = vec![
            (access_manager_pda, access_manager_account),
            (admin, create_signer_account()),
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
            ],
        );

        let accounts = vec![
            (access_manager_pda, access_manager_account),
            (non_admin, create_signer_account()),
        ];

        let mollusk = setup_mollusk();
        let checks = vec![Check::err(solana_sdk::program_error::ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + AccessManagerError::NotAdmin as u32,
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
            ],
        );

        let accounts = vec![
            (access_manager_pda, access_manager_account),
            (admin, create_signer_account()),
        ];

        let mollusk = setup_mollusk();
        let checks = vec![Check::err(solana_sdk::program_error::ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + AccessManagerError::InvalidRoleId as u32,
        ))];

        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }
}
