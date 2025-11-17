use crate::constants::*;
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

    /// CHECK: Validated via seeds constraint using stored `access_manager` program ID
    #[account(
        seeds = [access_manager::state::AccessManager::SEED],
        bump,
        seeds::program = app_state.access_manager
    )]
    pub access_manager: AccountInfo<'info>,

    pub authority: Signer<'info>,

    /// Instructions sysvar for CPI validation
    /// CHECK: Address constraint verifies this is the instructions sysvar
    #[account(address = anchor_lang::solana_program::sysvar::instructions::ID)]
    pub instructions_sysvar: AccountInfo<'info>,
}

pub fn pause_app(ctx: Context<PauseApp>) -> Result<()> {
    // Ethereum: ICS20Transfer.sol:116 - pause() restricted to PAUSER_ROLE
    // Performs: CPI rejection + signer verification + role check
    access_manager::require_role(
        &ctx.accounts.access_manager,
        solana_ibc_types::roles::PAUSER_ROLE,
        &ctx.accounts.authority,
        &ctx.accounts.instructions_sysvar,
        &crate::ID,
    )?;

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

    /// CHECK: Validated via seeds constraint using stored `access_manager` program ID using stored `access_manager` program ID
    #[account(
        seeds = [access_manager::state::AccessManager::SEED],
        bump,
        seeds::program = app_state.access_manager
    )]
    pub access_manager: AccountInfo<'info>,

    pub authority: Signer<'info>,

    /// Instructions sysvar for CPI validation
    /// CHECK: Address constraint verifies this is the instructions sysvar
    #[account(address = anchor_lang::solana_program::sysvar::instructions::ID)]
    pub instructions_sysvar: AccountInfo<'info>,
}

pub fn unpause_app(ctx: Context<UnpauseApp>) -> Result<()> {
    // Ethereum: ICS20Transfer.sol:121 - unpause() restricted to UNPAUSER_ROLE
    // Performs: CPI rejection + signer verification + role check
    access_manager::require_role(
        &ctx.accounts.access_manager,
        solana_ibc_types::roles::UNPAUSER_ROLE,
        &ctx.accounts.authority,
        &ctx.accounts.instructions_sysvar,
        &crate::ID,
    )?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{AccountVersion, GMPAppState};
    use crate::test_utils::*;
    use anchor_lang::InstructionData;
    use mollusk_svm::result::Check;
    use mollusk_svm::Mollusk;
    use solana_ibc_types::roles;
    use solana_sdk::program_error::ProgramError;
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

        let payer = Pubkey::new_unique();
        let (app_state_pda, _bump) =
            Pubkey::find_program_address(&[GMPAppState::SEED, GMP_PORT_ID.as_bytes()], &crate::ID);

        let instruction_data = crate::instruction::Initialize {
            access_manager: access_manager::ID,
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(app_state_pda, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(system_program::ID, false),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_pda_for_init(app_state_pda),
            create_payer_account(payer),
            create_system_program_account(),
            create_instructions_sysvar_account_with_caller(crate::ID),
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
        let (access_manager_pda, access_manager_account) =
            setup_access_manager_with_roles(&[(roles::PAUSER_ROLE, &[authority])]);
        let (app_state_pda, app_state_bump) =
            Pubkey::find_program_address(&[GMPAppState::SEED, GMP_PORT_ID.as_bytes()], &crate::ID);

        let instruction_data = crate::instruction::PauseApp {};

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(app_state_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new_readonly(authority, true),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_gmp_app_state_account(
                app_state_pda,
                app_state_bump,
                false, // not paused
            ),
            (access_manager_pda, access_manager_account),
            create_authority_account(authority),
            create_instructions_sysvar_account_with_caller(crate::ID),
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
        let (access_manager_pda, access_manager_account) =
            setup_access_manager_with_roles(&[(roles::UNPAUSER_ROLE, &[authority])]);
        let (app_state_pda, app_state_bump) =
            Pubkey::find_program_address(&[GMPAppState::SEED, GMP_PORT_ID.as_bytes()], &crate::ID);

        let app_state = GMPAppState {
            version: AccountVersion::V1,
            paused: true,
            bump: app_state_bump,
            access_manager: access_manager::ID,
            _reserved: [0; 256],
        };

        let mut data = Vec::new();
        data.extend_from_slice(GMPAppState::DISCRIMINATOR);
        app_state.serialize(&mut data).unwrap();

        let instruction_data = crate::instruction::UnpauseApp {};

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(app_state_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new_readonly(authority, true),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
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
            (access_manager_pda, access_manager_account),
            create_authority_account(authority),
            create_instructions_sysvar_account_with_caller(crate::ID),
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
        let (access_manager_pda, access_manager_account) =
            setup_access_manager_with_roles(&[(roles::PAUSER_ROLE, &[authority])]);
        let (app_state_pda, app_state_bump) =
            Pubkey::find_program_address(&[GMPAppState::SEED, GMP_PORT_ID.as_bytes()], &crate::ID);

        let instruction_data = crate::instruction::PauseApp {};

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(app_state_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new_readonly(wrong_authority, true),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_gmp_app_state_account(
                app_state_pda,
                app_state_bump,
                false, // not paused
            ),
            (access_manager_pda, access_manager_account),
            create_authority_account(wrong_authority),
            create_instructions_sysvar_account(),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "Pause app should fail with wrong authority"
        );
    }

    // ========================================================================

    #[test]
    fn test_pause_app_invalid_pda() {
        let mollusk = Mollusk::new(&crate::ID, crate::get_gmp_program_path());

        let authority = Pubkey::new_unique();
        let (access_manager_pda, access_manager_account) =
            setup_access_manager_with_roles(&[(roles::PAUSER_ROLE, &[authority])]);
        let (_correct_app_state_pda, _correct_bump) =
            Pubkey::find_program_address(&[GMPAppState::SEED, GMP_PORT_ID.as_bytes()], &crate::ID);

        // Use wrong PDA
        let wrong_app_state_pda = Pubkey::new_unique();

        let instruction_data = crate::instruction::PauseApp {};

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(wrong_app_state_pda, false), // Wrong PDA!
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new_readonly(authority, true),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            // Create account state at wrong PDA for testing
            create_gmp_app_state_account(
                wrong_app_state_pda,
                255u8,
                false, // not paused
            ),
            (access_manager_pda, access_manager_account),
            create_authority_account(authority),
            create_instructions_sysvar_account(),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "PauseApp should fail with invalid app_state PDA"
        );
    }

    #[test]
    fn test_pause_app_fake_sysvar_wormhole_attack() {
        let mollusk = Mollusk::new(&crate::ID, crate::get_gmp_program_path());

        let authority = Pubkey::new_unique();
        let (access_manager_pda, access_manager_account) =
            setup_access_manager_with_roles(&[(roles::PAUSER_ROLE, &[authority])]);
        let (app_state_pda, app_state_bump) =
            Pubkey::find_program_address(&[GMPAppState::SEED, GMP_PORT_ID.as_bytes()], &crate::ID);

        let instruction_data = crate::instruction::PauseApp {};

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(app_state_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new_readonly(authority, true),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
            data: instruction_data.data(),
        };

        // Replace real sysvar with fake one (Wormhole-style attack)
        let (instruction, fake_sysvar_account) = setup_fake_sysvar_attack(instruction, crate::ID);

        let accounts = vec![
            create_gmp_app_state_account(app_state_pda, app_state_bump, false),
            (access_manager_pda, access_manager_account),
            create_authority_account(authority),
            fake_sysvar_account,
        ];

        mollusk.process_and_validate_instruction(
            &instruction,
            &accounts,
            &[expect_sysvar_attack_error()],
        );
    }

    #[test]
    fn test_pause_app_cpi_rejection() {
        let mollusk = Mollusk::new(&crate::ID, crate::get_gmp_program_path());

        let authority = Pubkey::new_unique();
        let (access_manager_pda, access_manager_account) =
            setup_access_manager_with_roles(&[(roles::PAUSER_ROLE, &[authority])]);
        let (app_state_pda, app_state_bump) =
            Pubkey::find_program_address(&[GMPAppState::SEED, GMP_PORT_ID.as_bytes()], &crate::ID);

        let instruction_data = crate::instruction::PauseApp {};

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(app_state_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new_readonly(authority, true),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
            data: instruction_data.data(),
        };

        // Simulate CPI call from unauthorized program
        let malicious_program = Pubkey::new_unique();
        let (instruction, cpi_sysvar_account) = setup_cpi_call_test(instruction, malicious_program);

        let accounts = vec![
            create_gmp_app_state_account(app_state_pda, app_state_bump, false),
            (access_manager_pda, access_manager_account),
            create_authority_account(authority),
            cpi_sysvar_account,
        ];

        let checks = vec![Check::err(ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + access_manager::AccessManagerError::CpiNotAllowed as u32,
        ))];
        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }

    #[test]
    fn test_unpause_app_fake_sysvar_wormhole_attack() {
        let mollusk = Mollusk::new(&crate::ID, crate::get_gmp_program_path());

        let authority = Pubkey::new_unique();
        let (access_manager_pda, access_manager_account) =
            setup_access_manager_with_roles(&[(roles::UNPAUSER_ROLE, &[authority])]);
        let (app_state_pda, app_state_bump) =
            Pubkey::find_program_address(&[GMPAppState::SEED, GMP_PORT_ID.as_bytes()], &crate::ID);

        let instruction_data = crate::instruction::UnpauseApp {};

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(app_state_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new_readonly(authority, true),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
            data: instruction_data.data(),
        };

        // Replace real sysvar with fake one (Wormhole-style attack)
        let (instruction, fake_sysvar_account) = setup_fake_sysvar_attack(instruction, crate::ID);

        let accounts = vec![
            create_gmp_app_state_account(app_state_pda, app_state_bump, true), // paused
            (access_manager_pda, access_manager_account),
            create_authority_account(authority),
            fake_sysvar_account,
        ];

        mollusk.process_and_validate_instruction(
            &instruction,
            &accounts,
            &[expect_sysvar_attack_error()],
        );
    }

    #[test]
    fn test_unpause_app_cpi_rejection() {
        let mollusk = Mollusk::new(&crate::ID, crate::get_gmp_program_path());

        let authority = Pubkey::new_unique();
        let (access_manager_pda, access_manager_account) =
            setup_access_manager_with_roles(&[(roles::UNPAUSER_ROLE, &[authority])]);
        let (app_state_pda, app_state_bump) =
            Pubkey::find_program_address(&[GMPAppState::SEED, GMP_PORT_ID.as_bytes()], &crate::ID);

        let instruction_data = crate::instruction::UnpauseApp {};

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(app_state_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new_readonly(authority, true),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
            data: instruction_data.data(),
        };

        // Simulate CPI call from unauthorized program
        let malicious_program = Pubkey::new_unique();
        let (instruction, cpi_sysvar_account) = setup_cpi_call_test(instruction, malicious_program);

        let accounts = vec![
            create_gmp_app_state_account(app_state_pda, app_state_bump, true), // paused
            (access_manager_pda, access_manager_account),
            create_authority_account(authority),
            cpi_sysvar_account,
        ];

        let checks = vec![Check::err(ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + access_manager::AccessManagerError::CpiNotAllowed as u32,
        ))];
        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }
}
