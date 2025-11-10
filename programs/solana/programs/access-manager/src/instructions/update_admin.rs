use crate::events::AdminUpdatedEvent;
use crate::state::AccessManager;
use crate::types::AccessManagerError;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct UpdateAdmin<'info> {
    #[account(
        mut,
        seeds = [AccessManager::SEED],
        bump
    )]
    pub access_manager: Account<'info, AccessManager>,

    #[account(
        constraint = current_admin.key() == access_manager.admin @ AccessManagerError::NotAdmin
    )]
    pub current_admin: Signer<'info>,
}

pub fn update_admin(ctx: Context<UpdateAdmin>, new_admin: Pubkey) -> Result<()> {
    let access_manager = &mut ctx.accounts.access_manager;
    let old_admin = access_manager.admin;

    access_manager.admin = new_admin;

    emit!(AdminUpdatedEvent {
        old_admin,
        new_admin,
        updated_by: ctx.accounts.current_admin.key(),
    });

    msg!("Admin updated from {} to {}", old_admin, new_admin);

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
    fn test_update_admin_success() {
        let old_admin = Pubkey::new_unique();
        let new_admin = Pubkey::new_unique();

        let (access_manager_pda, access_manager_account) =
            create_initialized_access_manager(old_admin);

        let instruction = build_instruction(
            crate::instruction::UpdateAdmin { new_admin },
            vec![
                AccountMeta::new(access_manager_pda, false),
                AccountMeta::new_readonly(old_admin, true),
            ],
        );

        let accounts = vec![
            (access_manager_pda, access_manager_account),
            (old_admin, create_signer_account()),
        ];

        let mollusk = setup_mollusk();
        let checks = vec![Check::success()];
        let result = mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);

        let access_manager = get_access_manager_from_result(&result, &access_manager_pda);
        assert_eq!(access_manager.admin, new_admin);
    }

    #[test]
    fn test_update_admin_not_current_admin() {
        let current_admin = Pubkey::new_unique();
        let unauthorized = Pubkey::new_unique();
        let new_admin = Pubkey::new_unique();

        let (access_manager_pda, access_manager_account) =
            create_initialized_access_manager(current_admin);

        let instruction = build_instruction(
            crate::instruction::UpdateAdmin { new_admin },
            vec![
                AccountMeta::new(access_manager_pda, false),
                AccountMeta::new_readonly(unauthorized, true),
            ],
        );

        let accounts = vec![
            (access_manager_pda, access_manager_account),
            (unauthorized, create_signer_account()),
        ];

        let mollusk = setup_mollusk();
        let checks = vec![Check::err(solana_sdk::program_error::ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + AccessManagerError::NotAdmin as u32,
        ))];

        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }
}
