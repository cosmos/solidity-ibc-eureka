use crate::errors::RouterError;
use crate::events::RouterPausedEvent;
use crate::state::RouterState;
use anchor_lang::prelude::*;

/// Pauses the ICS26 router, halting all IBC packet processing.
/// Requires `PAUSER_ROLE` and rejects CPI calls.
#[derive(Accounts)]
pub struct Pause<'info> {
    /// Mutable global router configuration PDA whose `paused` flag will be set.
    #[account(
        mut,
        seeds = [RouterState::SEED],
        bump,
        constraint = !router_state.paused @ RouterError::RouterPaused,
    )]
    pub router_state: Account<'info, RouterState>,

    /// Global access control state used for pauser role verification.
    /// CHECK: Validated via seeds constraint using the stored `access_manager` program ID
    #[account(
        seeds = [access_manager::state::AccessManager::SEED],
        bump,
        seeds::program = router_state.access_manager,
    )]
    pub access_manager: AccountInfo<'info>,

    /// Signer authorized to pause the router.
    pub pauser: Signer<'info>,

    /// Instructions sysvar used for CPI detection.
    /// CHECK: Address constraint verifies this is the instructions sysvar
    #[account(address = anchor_lang::solana_program::sysvar::instructions::ID)]
    pub instructions_sysvar: AccountInfo<'info>,
}

pub fn pause(ctx: Context<Pause>) -> Result<()> {
    access_manager::require_role(
        &ctx.accounts.access_manager,
        solana_ibc_types::roles::PAUSER_ROLE,
        &ctx.accounts.pauser,
        &ctx.accounts.instructions_sysvar,
        &crate::ID,
    )?;

    ctx.accounts.router_state.paused = true;

    emit!(RouterPausedEvent {});

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

    fn build_pause_ix(pauser: Pubkey) -> solana_sdk::instruction::Instruction {
        let (router_state_pda, _) = Pubkey::find_program_address(&[RouterState::SEED], &crate::ID);
        let (access_manager_pda, _) = Pubkey::find_program_address(
            &[access_manager::state::AccessManager::SEED],
            &access_manager::ID,
        );

        build_instruction(
            crate::instruction::Pause {},
            vec![
                AccountMeta::new(router_state_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new_readonly(pauser, true),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
        )
    }

    #[test]
    fn test_pause_success() {
        let pauser = Pubkey::new_unique();

        let (router_state_pda, router_state_account) = create_initialized_router_state();

        let (access_manager_pda, access_manager_account) =
            create_access_manager_with_role(pauser, roles::PAUSER_ROLE, pauser);

        let instruction = build_pause_ix(pauser);

        let accounts = vec![
            (router_state_pda, router_state_account),
            (access_manager_pda, access_manager_account),
            (pauser, create_signer_account()),
            create_instructions_sysvar_account_with_caller(crate::ID),
        ];

        let mollusk = setup_mollusk();
        let result =
            mollusk.process_and_validate_instruction(&instruction, &accounts, &[Check::success()]);

        let router_state = get_router_state_from_result(&result, &router_state_pda);
        assert!(router_state.paused);
    }

    #[test]
    fn test_pause_unauthorized() {
        let pauser = Pubkey::new_unique();
        let non_pauser = Pubkey::new_unique();

        let (router_state_pda, router_state_account) = create_initialized_router_state();

        let (access_manager_pda, access_manager_account) =
            create_access_manager_with_role(pauser, roles::PAUSER_ROLE, pauser);

        let instruction = build_pause_ix(non_pauser);

        let accounts = vec![
            (router_state_pda, router_state_account),
            (access_manager_pda, access_manager_account),
            (non_pauser, create_signer_account()),
            create_instructions_sysvar_account_with_caller(crate::ID),
        ];

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
    fn test_pause_already_paused() {
        let pauser = Pubkey::new_unique();

        let (router_state_pda, router_state_account) = create_initialized_paused_router_state();

        let (access_manager_pda, access_manager_account) =
            create_access_manager_with_role(pauser, roles::PAUSER_ROLE, pauser);

        let instruction = build_pause_ix(pauser);

        let accounts = vec![
            (router_state_pda, router_state_account),
            (access_manager_pda, access_manager_account),
            (pauser, create_signer_account()),
            create_instructions_sysvar_account_with_caller(crate::ID),
        ];

        let mollusk = setup_mollusk();
        mollusk.process_and_validate_instruction(
            &instruction,
            &accounts,
            &[Check::err(solana_sdk::program_error::ProgramError::Custom(
                ANCHOR_ERROR_OFFSET + RouterError::RouterPaused as u32,
            ))],
        );
    }

    #[test]
    fn test_pause_cpi_rejection() {
        let pauser = Pubkey::new_unique();

        let (router_state_pda, router_state_account) = create_initialized_router_state();

        let (access_manager_pda, access_manager_account) =
            create_access_manager_with_role(pauser, roles::PAUSER_ROLE, pauser);

        let instruction = build_pause_ix(pauser);

        let malicious_program = Pubkey::new_unique();
        let (instruction, cpi_sysvar_account) = setup_cpi_call_test(instruction, malicious_program);

        let accounts = vec![
            (router_state_pda, router_state_account),
            (access_manager_pda, access_manager_account),
            (pauser, create_signer_account()),
            cpi_sysvar_account,
        ];

        let mollusk = setup_mollusk();
        mollusk.process_and_validate_instruction(
            &instruction,
            &accounts,
            &[Check::err(solana_sdk::program_error::ProgramError::Custom(
                ANCHOR_ERROR_OFFSET + AccessManagerError::CpiNotAllowed as u32,
            ))],
        );
    }

    #[test]
    fn test_pause_fake_sysvar_attack() {
        let pauser = Pubkey::new_unique();

        let (router_state_pda, router_state_account) = create_initialized_router_state();

        let (access_manager_pda, access_manager_account) =
            create_access_manager_with_role(pauser, roles::PAUSER_ROLE, pauser);

        let instruction = build_pause_ix(pauser);

        let (instruction, fake_sysvar_account) = setup_fake_sysvar_attack(instruction, crate::ID);

        let accounts = vec![
            (router_state_pda, router_state_account),
            (access_manager_pda, access_manager_account),
            (pauser, create_signer_account()),
            fake_sysvar_account,
        ];

        let mollusk = setup_mollusk();
        mollusk.process_and_validate_instruction(
            &instruction,
            &accounts,
            &[expect_sysvar_attack_error()],
        );
    }
}
