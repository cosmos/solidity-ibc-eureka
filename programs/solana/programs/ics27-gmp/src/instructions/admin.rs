use crate::constants::*;
use crate::errors::GMPError;
use crate::events::{GMPAppPaused, GMPAppUnpaused};
use crate::state::GMPAppState;
use anchor_lang::prelude::*;

/// Pause the entire GMP app (admin only)
#[derive(Accounts)]
pub struct PauseApp<'info> {
    /// App state account - validated by Anchor PDA constraints
    #[account(
        mut,
        seeds = [GMPAppState::SEED, GMP_PORT_ID.as_bytes()],
        bump = app_state.bump
    )]
    pub app_state: Account<'info, GMPAppState>,

    #[account(
        constraint = authority.key() == app_state.authority @ GMPError::UnauthorizedAdmin
    )]
    pub authority: Signer<'info>,
}

pub fn pause_app(ctx: Context<PauseApp>) -> Result<()> {
    let clock = Clock::get()?;
    let app_state = &mut ctx.accounts.app_state;

    app_state.paused = true;

    emit!(GMPAppPaused {
        admin: ctx.accounts.authority.key(),
        timestamp: clock.unix_timestamp,
    });

    msg!("GMP app paused by admin: {}", ctx.accounts.authority.key());

    Ok(())
}

/// Unpause the entire GMP app (admin only)
#[derive(Accounts)]
pub struct UnpauseApp<'info> {
    /// App state account - validated by Anchor PDA constraints
    #[account(
        mut,
        seeds = [GMPAppState::SEED, GMP_PORT_ID.as_bytes()],
        bump = app_state.bump
    )]
    pub app_state: Account<'info, GMPAppState>,

    #[account(
        constraint = authority.key() == app_state.authority @ GMPError::UnauthorizedAdmin
    )]
    pub authority: Signer<'info>,
}

pub fn unpause_app(ctx: Context<UnpauseApp>) -> Result<()> {
    let clock = Clock::get()?;
    let app_state = &mut ctx.accounts.app_state;

    app_state.paused = false;

    emit!(GMPAppUnpaused {
        admin: ctx.accounts.authority.key(),
        timestamp: clock.unix_timestamp,
    });

    msg!(
        "GMP app unpaused by admin: {}",
        ctx.accounts.authority.key()
    );

    Ok(())
}

/// Update app authority (admin only)
#[derive(Accounts)]
pub struct UpdateAuthority<'info> {
    /// App state account - validated by Anchor PDA constraints
    #[account(
        mut,
        seeds = [GMPAppState::SEED, GMP_PORT_ID.as_bytes()],
        bump = app_state.bump
    )]
    pub app_state: Account<'info, GMPAppState>,

    #[account(
        constraint = current_authority.key() == app_state.authority @ GMPError::UnauthorizedAdmin
    )]
    pub current_authority: Signer<'info>,

    /// CHECK: New authority can be any valid Pubkey
    pub new_authority: AccountInfo<'info>,
}

pub fn update_authority(ctx: Context<UpdateAuthority>) -> Result<()> {
    let app_state = &mut ctx.accounts.app_state;
    let old_authority = app_state.authority;

    app_state.authority = ctx.accounts.new_authority.key();

    msg!(
        "GMP app authority updated: {} -> {}",
        old_authority,
        app_state.authority
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::GMPAppState;
    use crate::test_utils::*;
    use anchor_lang::InstructionData;
    use mollusk_svm::Mollusk;
    use solana_sdk::{
        account::Account as SolanaAccount,
        instruction::{AccountMeta, Instruction},
        pubkey::Pubkey,
        system_program,
    };

    // ========================================================================
    // Initialize Tests
    // ========================================================================

    #[test]
    fn test_initialize_success() {
        let mollusk = Mollusk::new(&crate::ID, crate::get_gmp_program_path());

        let router_program = Pubkey::new_unique();
        let authority = Pubkey::new_unique();
        let (app_state_pda, _bump) =
            Pubkey::find_program_address(&[GMPAppState::SEED, GMP_PORT_ID.as_bytes()], &crate::ID);
        let (router_caller_pda, _) = Pubkey::find_program_address(&[b"router_caller"], &crate::ID);
        let payer = authority;

        let instruction_data = crate::instruction::Initialize { router_program };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(app_state_pda, false),
                AccountMeta::new(router_caller_pda, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(authority, true),
                AccountMeta::new_readonly(system_program::ID, false),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_pda_for_init(app_state_pda),
            create_pda_for_init(router_caller_pda),
            create_payer_account(payer),
            create_system_program_account(),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(!result.program_result.is_err(), "Initialize should succeed");
    }

    // ========================================================================
    // Pause/Unpause App Tests
    // ========================================================================

    #[test]
    fn test_pause_app_success() {
        let mollusk = Mollusk::new(&crate::ID, crate::get_gmp_program_path());

        let authority = Pubkey::new_unique();
        let router_program = Pubkey::new_unique();

        let (app_state_pda, app_state_bump) =
            Pubkey::find_program_address(&[GMPAppState::SEED, GMP_PORT_ID.as_bytes()], &crate::ID);

        let instruction_data = crate::instruction::PauseApp {};

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(app_state_pda, false),
                AccountMeta::new_readonly(authority, true),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_gmp_app_state_account(
                app_state_pda,
                router_program,
                authority,
                app_state_bump,
                false, // not paused
            ),
            create_authority_account(authority),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(
            !result.program_result.is_err(),
            "Pause app should succeed: {:?}",
            result.program_result
        );

        let app_state_account = result.get_account(&app_state_pda).unwrap();
        let app_state_data = &app_state_account.data[crate::constants::DISCRIMINATOR_SIZE..];
        let app_state = GMPAppState::try_from_slice(app_state_data).unwrap();
        assert!(app_state.paused, "App should be paused");
    }

    #[test]
    fn test_unpause_app_success() {
        let mollusk = Mollusk::new(&crate::ID, crate::get_gmp_program_path());

        let authority = Pubkey::new_unique();
        let router_program = Pubkey::new_unique();

        let (app_state_pda, app_state_bump) =
            Pubkey::find_program_address(&[GMPAppState::SEED, GMP_PORT_ID.as_bytes()], &crate::ID);

        let app_state = GMPAppState {
            router_program,
            authority,
            version: 1,
            paused: true,
            bump: app_state_bump,
        };

        let mut data = Vec::new();
        data.extend_from_slice(GMPAppState::DISCRIMINATOR);
        app_state.serialize(&mut data).unwrap();

        let instruction_data = crate::instruction::UnpauseApp {};

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(app_state_pda, false),
                AccountMeta::new_readonly(authority, true),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            (
                app_state_pda,
                SolanaAccount {
                    lamports: 1_000_000,
                    data,
                    owner: crate::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            create_authority_account(authority),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(
            !result.program_result.is_err(),
            "Unpause app should succeed: {:?}",
            result.program_result
        );

        let app_state_account = result.get_account(&app_state_pda).unwrap();
        let app_state_data = &app_state_account.data[crate::constants::DISCRIMINATOR_SIZE..];
        let app_state = GMPAppState::try_from_slice(app_state_data).unwrap();
        assert!(!app_state.paused, "App should be unpaused");
    }

    #[test]
    fn test_pause_app_unauthorized() {
        let mollusk = Mollusk::new(&crate::ID, crate::get_gmp_program_path());

        let authority = Pubkey::new_unique();
        let wrong_authority = Pubkey::new_unique();
        let router_program = Pubkey::new_unique();

        let (app_state_pda, app_state_bump) =
            Pubkey::find_program_address(&[GMPAppState::SEED, GMP_PORT_ID.as_bytes()], &crate::ID);

        let instruction_data = crate::instruction::PauseApp {};

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(app_state_pda, false),
                AccountMeta::new_readonly(wrong_authority, true),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_gmp_app_state_account(
                app_state_pda,
                router_program,
                authority,
                app_state_bump,
                false, // not paused
            ),
            create_authority_account(wrong_authority),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "Pause app should fail with wrong authority"
        );
    }

    // ========================================================================
    // Update Authority Tests
    // ========================================================================

    #[test]
    fn test_update_authority_success() {
        let mollusk = Mollusk::new(&crate::ID, crate::get_gmp_program_path());

        let current_authority = Pubkey::new_unique();
        let new_authority = Pubkey::new_unique();
        let router_program = Pubkey::new_unique();

        let (app_state_pda, app_state_bump) =
            Pubkey::find_program_address(&[GMPAppState::SEED, GMP_PORT_ID.as_bytes()], &crate::ID);

        let instruction_data = crate::instruction::UpdateAuthority {};

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(app_state_pda, false),
                AccountMeta::new_readonly(current_authority, true),
                AccountMeta::new_readonly(new_authority, false),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_gmp_app_state_account(
                app_state_pda,
                router_program,
                current_authority,
                app_state_bump,
                false, // not paused
            ),
            create_authority_account(current_authority),
            create_authority_account(new_authority),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(
            !result.program_result.is_err(),
            "Update authority should succeed: {:?}",
            result.program_result
        );

        let app_state_account = result.get_account(&app_state_pda).unwrap();
        let app_state_data = &app_state_account.data[crate::constants::DISCRIMINATOR_SIZE..];
        let app_state = GMPAppState::try_from_slice(app_state_data).unwrap();
        assert_eq!(
            app_state.authority, new_authority,
            "Authority should be updated"
        );
    }

    #[test]
    fn test_update_authority_unauthorized() {
        let mollusk = Mollusk::new(&crate::ID, crate::get_gmp_program_path());

        let current_authority = Pubkey::new_unique();
        let wrong_authority = Pubkey::new_unique();
        let new_authority = Pubkey::new_unique();
        let router_program = Pubkey::new_unique();

        let (app_state_pda, app_state_bump) =
            Pubkey::find_program_address(&[GMPAppState::SEED, GMP_PORT_ID.as_bytes()], &crate::ID);

        let instruction_data = crate::instruction::UpdateAuthority {};

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(app_state_pda, false),
                AccountMeta::new_readonly(wrong_authority, true),
                AccountMeta::new_readonly(new_authority, false),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_gmp_app_state_account(
                app_state_pda,
                router_program,
                current_authority,
                app_state_bump,
                false, // not paused
            ),
            create_authority_account(wrong_authority),
            create_authority_account(new_authority),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "Update authority should fail with wrong authority"
        );
    }

    #[test]
    fn test_pause_app_invalid_pda() {
        let mollusk = Mollusk::new(&crate::ID, crate::get_gmp_program_path());

        let authority = Pubkey::new_unique();
        let router_program = Pubkey::new_unique();

        let (_correct_app_state_pda, _correct_bump) =
            Pubkey::find_program_address(&[GMPAppState::SEED, GMP_PORT_ID.as_bytes()], &crate::ID);

        // Use wrong PDA
        let wrong_app_state_pda = Pubkey::new_unique();

        let instruction_data = crate::instruction::PauseApp {};

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(wrong_app_state_pda, false), // Wrong PDA!
                AccountMeta::new_readonly(authority, true),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            // Create account state at wrong PDA for testing
            create_gmp_app_state_account(
                wrong_app_state_pda,
                router_program,
                authority,
                255u8,
                false, // not paused
            ),
            create_authority_account(authority),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "PauseApp should fail with invalid app_state PDA"
        );
    }
}
