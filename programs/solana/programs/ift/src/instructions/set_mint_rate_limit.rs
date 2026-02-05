use anchor_lang::prelude::*;

use crate::constants::*;
use crate::events::MintRateLimitUpdated;
use crate::state::{IFTAppState, SetMintRateLimitMsg};

#[derive(Accounts)]
pub struct SetMintRateLimit<'info> {
    #[account(
        mut,
        seeds = [IFT_APP_STATE_SEED, app_state.mint.as_ref()],
        bump = app_state.bump
    )]
    pub app_state: Account<'info, IFTAppState>,

    /// CHECK: Validated via seeds constraint using stored `access_manager` program ID
    #[account(
        seeds = [access_manager::state::AccessManager::SEED],
        bump,
        seeds::program = app_state.access_manager
    )]
    pub access_manager: AccountInfo<'info>,

    /// Admin with admin role
    pub admin: Signer<'info>,

    /// Instructions sysvar for CPI validation
    /// CHECK: Address constraint verifies this is the instructions sysvar
    #[account(address = anchor_lang::solana_program::sysvar::instructions::ID)]
    pub instructions_sysvar: AccountInfo<'info>,
}

pub fn set_mint_rate_limit(ctx: Context<SetMintRateLimit>, msg: SetMintRateLimitMsg) -> Result<()> {
    access_manager::require_role(
        &ctx.accounts.access_manager,
        solana_ibc_types::roles::ADMIN_ROLE,
        &ctx.accounts.admin,
        &ctx.accounts.instructions_sysvar,
        &crate::ID,
    )?;

    ctx.accounts.app_state.daily_mint_limit = msg.daily_mint_limit;

    let clock = Clock::get()?;
    emit!(MintRateLimitUpdated {
        mint: ctx.accounts.app_state.mint,
        daily_mint_limit: msg.daily_mint_limit,
        timestamp: clock.unix_timestamp,
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use anchor_lang::InstructionData;
    use rstest::rstest;
    use solana_sdk::{
        instruction::{AccountMeta, Instruction},
        pubkey::Pubkey,
    };

    use crate::state::SetMintRateLimitMsg;
    use crate::test_utils::*;

    fn run_set_mint_rate_limit_success_test(limit: u64) {
        let mollusk = setup_mollusk();

        let mint = Pubkey::new_unique();
        let admin = Pubkey::new_unique();
        let (app_state_pda, app_state_bump) = get_app_state_pda(&mint);
        let (_, mint_authority_bump) = get_mint_authority_pda(&mint);
        let (access_manager_pda, access_manager_account) =
            create_access_manager_account_with_admin(admin);
        let (instructions_sysvar, instructions_account) = create_instructions_sysvar_account();

        let app_state_account = create_ift_app_state_account(
            mint,
            app_state_bump,
            mint_authority_bump,
            access_manager::ID,
            Pubkey::new_unique(),
        );

        let msg = SetMintRateLimitMsg {
            daily_mint_limit: limit,
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(app_state_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new_readonly(admin, true),
                AccountMeta::new_readonly(instructions_sysvar, false),
            ],
            data: crate::instruction::SetMintRateLimit { msg }.data(),
        };

        let accounts = vec![
            (app_state_pda, app_state_account),
            (access_manager_pda, access_manager_account),
            (admin, create_signer_account()),
            (instructions_sysvar, instructions_account),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(
            !result.program_result.is_err(),
            "set_mint_rate_limit({limit}) should succeed: {:?}",
            result.program_result
        );

        let updated_account = result
            .resulting_accounts
            .iter()
            .find(|(k, _)| *k == app_state_pda)
            .expect("app state should exist")
            .1
            .clone();
        let updated_state = deserialize_app_state(&updated_account);
        assert_eq!(updated_state.daily_mint_limit, limit);
    }

    #[rstest]
    #[case::set_limit(1_000_000)]
    #[case::disable(0)]
    fn test_set_mint_rate_limit_success(#[case] limit: u64) {
        run_set_mint_rate_limit_success_test(limit);
    }

    #[derive(Clone, Copy)]
    enum SetMintRateLimitErrorCase {
        Unauthorized,
        FakeSysvarAttack,
        CpiRejection,
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum SysvarMode {
        Normal,
        FakeSysvar,
        CpiCall,
    }

    fn run_set_mint_rate_limit_error_test(case: SetMintRateLimitErrorCase) {
        let mollusk = setup_mollusk();

        let mint = Pubkey::new_unique();
        let admin = Pubkey::new_unique();
        let unauthorized = Pubkey::new_unique();

        let (use_unauthorized, sysvar_mode) = match case {
            SetMintRateLimitErrorCase::Unauthorized => (true, SysvarMode::Normal),
            SetMintRateLimitErrorCase::FakeSysvarAttack => (false, SysvarMode::FakeSysvar),
            SetMintRateLimitErrorCase::CpiRejection => (false, SysvarMode::CpiCall),
        };

        let (app_state_pda, app_state_bump) = get_app_state_pda(&mint);
        let (_, mint_authority_bump) = get_mint_authority_pda(&mint);
        let (access_manager_pda, access_manager_account) =
            create_access_manager_account_with_admin(admin);

        let (instructions_sysvar, instructions_account) = match sysvar_mode {
            SysvarMode::Normal => create_instructions_sysvar_account(),
            SysvarMode::FakeSysvar => create_fake_instructions_sysvar_account(admin),
            SysvarMode::CpiCall => {
                create_instructions_sysvar_account_with_caller(Pubkey::new_unique())
            }
        };

        let app_state_account = create_ift_app_state_account(
            mint,
            app_state_bump,
            mint_authority_bump,
            access_manager::ID,
            Pubkey::new_unique(),
        );

        let signer = if use_unauthorized {
            unauthorized
        } else {
            admin
        };

        let msg = SetMintRateLimitMsg {
            daily_mint_limit: 1_000_000,
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(app_state_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new_readonly(signer, true),
                AccountMeta::new_readonly(instructions_sysvar, false),
            ],
            data: crate::instruction::SetMintRateLimit { msg }.data(),
        };

        let accounts = vec![
            (app_state_pda, app_state_account),
            (access_manager_pda, access_manager_account),
            (signer, create_signer_account()),
            (instructions_sysvar, instructions_account),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(result.program_result.is_err());
    }

    #[rstest]
    #[case::unauthorized(SetMintRateLimitErrorCase::Unauthorized)]
    #[case::fake_sysvar_attack(SetMintRateLimitErrorCase::FakeSysvarAttack)]
    #[case::cpi_rejection(SetMintRateLimitErrorCase::CpiRejection)]
    fn test_set_mint_rate_limit_validation(#[case] case: SetMintRateLimitErrorCase) {
        run_set_mint_rate_limit_error_test(case);
    }
}
