use anchor_lang::prelude::*;

use crate::constants::*;
use crate::events::IFTInitialized;
use crate::state::{AccountVersion, IFTAppState};

#[derive(Accounts)]
pub struct Initialize<'info> {
    /// Global IFT app state PDA (to be created, singleton)
    #[account(
        init,
        payer = payer,
        space = 8 + IFTAppState::INIT_SPACE,
        seeds = [IFT_APP_STATE_SEED],
        bump
    )]
    pub app_state: Account<'info, IFTAppState>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn initialize(ctx: Context<Initialize>, admin: Pubkey, gmp_program: Pubkey) -> Result<()> {
    let app_state = &mut ctx.accounts.app_state;
    app_state.version = AccountVersion::V1;
    app_state.bump = ctx.bumps.app_state;
    app_state.admin = admin;
    app_state.gmp_program = gmp_program;

    let clock = Clock::get()?;
    emit!(IFTInitialized {
        admin,
        gmp_program,
        timestamp: clock.unix_timestamp,
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use anchor_lang::{InstructionData, Space};
    use solana_sdk::{
        instruction::{AccountMeta, Instruction},
        pubkey::Pubkey,
        rent::Rent,
    };

    use crate::state::IFTAppState;
    use crate::test_utils::*;

    #[test]
    fn test_initialize_success() {
        let mollusk = setup_mollusk();

        let admin = Pubkey::new_unique();
        let gmp_program = Pubkey::new_unique();
        let payer = Pubkey::new_unique();

        let (app_state_pda, _) = get_app_state_pda();
        let (system_program, system_account) = create_system_program_account();

        let app_state_account = solana_sdk::account::Account {
            lamports: Rent::default().minimum_balance(8 + IFTAppState::INIT_SPACE),
            data: vec![],
            owner: solana_sdk::system_program::ID,
            executable: false,
            rent_epoch: 0,
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(app_state_pda, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(system_program, false),
            ],
            data: crate::instruction::Initialize { admin, gmp_program }.data(),
        };

        let accounts = vec![
            (app_state_pda, app_state_account),
            (payer, create_signer_account()),
            (system_program, system_account),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(
            !result.program_result.is_err(),
            "initialize should succeed: {:?}",
            result.program_result
        );

        let updated_account = result
            .resulting_accounts
            .iter()
            .find(|(k, _)| *k == app_state_pda)
            .expect("app state should exist")
            .1
            .clone();
        let state = deserialize_app_state(&updated_account);
        assert_eq!(state.admin, admin);
        assert_eq!(state.gmp_program, gmp_program);
        assert!(!state.paused);
    }

    #[test]
    fn test_initialize_already_initialized_fails() {
        let mollusk = setup_mollusk();

        let admin = Pubkey::new_unique();
        let gmp_program = Pubkey::new_unique();
        let payer = Pubkey::new_unique();

        let (app_state_pda, app_state_bump) = get_app_state_pda();
        let (system_program, system_account) = create_system_program_account();

        let app_state_account = create_ift_app_state_account(app_state_bump, admin, gmp_program);

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(app_state_pda, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(system_program, false),
            ],
            data: crate::instruction::Initialize { admin, gmp_program }.data(),
        };

        let accounts = vec![
            (app_state_pda, app_state_account),
            (payer, create_signer_account()),
            (system_program, system_account),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        // System program returns AccountAlreadyInUse (0) when init tries to
        // create_account for an account that already has data/lamports
        assert_eq!(
            result.program_result,
            Err(solana_sdk::instruction::InstructionError::Custom(0)).into(),
        );
    }
}
