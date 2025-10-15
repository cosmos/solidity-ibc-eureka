use crate::constants::*;
use crate::errors::GMPError;
use crate::events::{GMPAppPaused, GMPAppUnpaused};
use crate::state::GMPAppState;
use anchor_lang::prelude::*;

/// Pause the entire GMP app (admin only)
#[derive(Accounts)]
pub struct PauseApp<'info> {
    /// App state account - PDA validation done in handler
    #[account(mut)]
    pub app_state: Account<'info, GMPAppState>,

    #[account(
        constraint = authority.key() == app_state.authority @ GMPError::UnauthorizedAdmin
    )]
    pub authority: Signer<'info>,
}

pub fn pause_app(ctx: Context<PauseApp>) -> Result<()> {
    // Get clock directly via syscall
    let clock = Clock::get()?;

    // Validate app_state PDA using port_id from state
    let (expected_app_state_pda, _bump) = Pubkey::find_program_address(
        &[
            GMP_APP_STATE_SEED,
            ctx.accounts.app_state.port_id.as_bytes(),
        ],
        ctx.program_id,
    );
    require!(
        ctx.accounts.app_state.key() == expected_app_state_pda,
        GMPError::InvalidAccountAddress
    );

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
    /// App state account - PDA validation done in handler
    #[account(mut)]
    pub app_state: Account<'info, GMPAppState>,

    #[account(
        constraint = authority.key() == app_state.authority @ GMPError::UnauthorizedAdmin
    )]
    pub authority: Signer<'info>,
}

pub fn unpause_app(ctx: Context<UnpauseApp>) -> Result<()> {
    // Get clock directly via syscall
    let clock = Clock::get()?;

    // Validate app_state PDA using port_id from state
    let (expected_app_state_pda, _bump) = Pubkey::find_program_address(
        &[
            GMP_APP_STATE_SEED,
            ctx.accounts.app_state.port_id.as_bytes(),
        ],
        ctx.program_id,
    );
    require!(
        ctx.accounts.app_state.key() == expected_app_state_pda,
        GMPError::InvalidAccountAddress
    );

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
    /// App state account - PDA validation done in handler
    #[account(mut)]
    pub app_state: Account<'info, GMPAppState>,

    #[account(
        constraint = current_authority.key() == app_state.authority @ GMPError::UnauthorizedAdmin
    )]
    pub current_authority: Signer<'info>,

    /// CHECK: New authority can be any valid Pubkey
    pub new_authority: AccountInfo<'info>,
}

pub fn update_authority(ctx: Context<UpdateAuthority>) -> Result<()> {
    // Validate app_state PDA using port_id from state
    let (expected_app_state_pda, _bump) = Pubkey::find_program_address(
        &[
            GMP_APP_STATE_SEED,
            ctx.accounts.app_state.port_id.as_bytes(),
        ],
        ctx.program_id,
    );
    require!(
        ctx.accounts.app_state.key() == expected_app_state_pda,
        GMPError::InvalidAccountAddress
    );

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
    use crate::state::AccountState;
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
        let port_id = "gmpport".to_string();
        let (app_state_pda, _bump) = Pubkey::find_program_address(
            &[crate::constants::GMP_APP_STATE_SEED, port_id.as_bytes()],
            &crate::ID,
        );
        let (router_caller_pda, _) = Pubkey::find_program_address(&[b"router_caller"], &crate::ID);
        let payer = authority;

        let instruction_data = crate::instruction::Initialize {
            router_program,
            port_id,
        };

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

    #[test]
    fn test_initialize_invalid_port_id() {
        let mollusk = Mollusk::new(&crate::ID, crate::get_gmp_program_path());

        let router_program = Pubkey::new_unique();
        let authority = Pubkey::new_unique();
        let port_id_too_long = "a".repeat(crate::constants::MAX_PORT_ID_LENGTH + 1);

        let valid_port_id = "gmp";
        let (app_state_pda, _bump) = Pubkey::find_program_address(
            &[
                crate::constants::GMP_APP_STATE_SEED,
                valid_port_id.as_bytes(),
            ],
            &crate::ID,
        );
        let (router_caller_pda, _) = Pubkey::find_program_address(&[b"router_caller"], &crate::ID);
        let payer = authority;

        let instruction_data = crate::instruction::Initialize {
            router_program,
            port_id: port_id_too_long,
        };

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
        assert!(
            result.program_result.is_err(),
            "Should fail with port ID too long"
        );
    }

    // ========================================================================
    // Pause/Unpause App Tests
    // ========================================================================

    #[test]
    fn test_pause_app_success() {
        let mollusk = Mollusk::new(&crate::ID, crate::get_gmp_program_path());

        let authority = Pubkey::new_unique();
        let router_program = Pubkey::new_unique();
        let port_id = "gmpport".to_string();

        let (app_state_pda, app_state_bump) = Pubkey::find_program_address(
            &[crate::constants::GMP_APP_STATE_SEED, port_id.as_bytes()],
            &crate::ID,
        );

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
                port_id,
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
        let app_state_data = &app_state_account.data[8..];
        let app_state = GMPAppState::try_from_slice(app_state_data).unwrap();
        assert!(app_state.paused, "App should be paused");
    }

    #[test]
    fn test_unpause_app_success() {
        let mollusk = Mollusk::new(&crate::ID, crate::get_gmp_program_path());

        let authority = Pubkey::new_unique();
        let router_program = Pubkey::new_unique();
        let port_id = "gmpport".to_string();

        let (app_state_pda, app_state_bump) = Pubkey::find_program_address(
            &[crate::constants::GMP_APP_STATE_SEED, port_id.as_bytes()],
            &crate::ID,
        );

        let app_state = GMPAppState {
            router_program,
            port_id,
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
        let app_state_data = &app_state_account.data[8..];
        let app_state = GMPAppState::try_from_slice(app_state_data).unwrap();
        assert!(!app_state.paused, "App should be unpaused");
    }

    #[test]
    fn test_pause_app_unauthorized() {
        let mollusk = Mollusk::new(&crate::ID, crate::get_gmp_program_path());

        let authority = Pubkey::new_unique();
        let wrong_authority = Pubkey::new_unique();
        let router_program = Pubkey::new_unique();
        let port_id = "gmpport".to_string();

        let (app_state_pda, app_state_bump) = Pubkey::find_program_address(
            &[crate::constants::GMP_APP_STATE_SEED, port_id.as_bytes()],
            &crate::ID,
        );

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
                port_id,
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

        let authority = Pubkey::new_unique();
        let router_program = Pubkey::new_unique();
        let port_id = "gmpport".to_string();
        let client_id = "cosmoshub-1".to_string();
        let sender = "cosmos1test".to_string();
        let salt = vec![1, 2, 3];

        let (app_state_pda, app_state_bump) = Pubkey::find_program_address(
            &[crate::constants::GMP_APP_STATE_SEED, port_id.as_bytes()],
            &crate::ID,
        );

        let (account_pda, account_bump) =
            AccountState::derive_address(&client_id, &sender, &salt, &crate::ID).unwrap();

        let instruction_data = crate::instruction::FreezeAccount {
            client_id: client_id.clone(),
            sender: sender.clone(),
            salt: salt.clone(),
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new_readonly(app_state_pda, false),
                AccountMeta::new(account_pda, false),
                AccountMeta::new_readonly(authority, true),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_gmp_app_state_account(
                app_state_pda,
                router_program,
                port_id,
                authority,
                app_state_bump,
                false, // not paused
            ),
            create_account_state_with_nonce(
                account_pda,
                client_id,
                sender,
                salt,
                0,
                false,
                account_bump,
            ),
            create_authority_account(authority),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(
            !result.program_result.is_err(),
            "Freeze account should succeed: {:?}",
            result.program_result
        );

        let account_state_account = result.get_account(&account_pda).unwrap();
        let account_state_data = &account_state_account.data[8..];
        let account_state = AccountState::try_from_slice(account_state_data).unwrap();
        assert!(account_state.frozen, "Account should be frozen");
    }

    #[test]
    fn test_unfreeze_account_success() {
        let mollusk = Mollusk::new(&crate::ID, crate::get_gmp_program_path());

        let authority = Pubkey::new_unique();
        let router_program = Pubkey::new_unique();
        let port_id = "gmpport".to_string();
        let client_id = "cosmoshub-1".to_string();
        let sender = "cosmos1test".to_string();
        let salt = vec![1, 2, 3];

        let (app_state_pda, app_state_bump) = Pubkey::find_program_address(
            &[crate::constants::GMP_APP_STATE_SEED, port_id.as_bytes()],
            &crate::ID,
        );

        let (account_pda, account_bump) =
            AccountState::derive_address(&client_id, &sender, &salt, &crate::ID).unwrap();

        let instruction_data = crate::instruction::UnfreezeAccount {
            client_id: client_id.clone(),
            sender: sender.clone(),
            salt: salt.clone(),
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new_readonly(app_state_pda, false),
                AccountMeta::new(account_pda, false),
                AccountMeta::new_readonly(authority, true),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_gmp_app_state_account(
                app_state_pda,
                router_program,
                port_id,
                authority,
                app_state_bump,
                false, // not paused
            ),
            create_account_state_with_nonce(
                account_pda,
                client_id,
                sender,
                salt,
                0,
                true,
                account_bump,
            ),
            create_authority_account(authority),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(
            !result.program_result.is_err(),
            "Unfreeze account should succeed: {:?}",
            result.program_result
        );

        let account_state_account = result.get_account(&account_pda).unwrap();
        let account_state_data = &account_state_account.data[8..];
        let account_state = AccountState::try_from_slice(account_state_data).unwrap();
        assert!(!account_state.frozen, "Account should be unfrozen");
    }

    #[test]
    fn test_freeze_account_unauthorized() {
        let mollusk = Mollusk::new(&crate::ID, crate::get_gmp_program_path());

        let authority = Pubkey::new_unique();
        let wrong_authority = Pubkey::new_unique();
        let router_program = Pubkey::new_unique();
        let port_id = "gmpport".to_string();
        let client_id = "cosmoshub-1".to_string();
        let sender = "cosmos1test".to_string();
        let salt = vec![1, 2, 3];

        let (app_state_pda, app_state_bump) = Pubkey::find_program_address(
            &[crate::constants::GMP_APP_STATE_SEED, port_id.as_bytes()],
            &crate::ID,
        );

        let (account_pda, account_bump) =
            AccountState::derive_address(&client_id, &sender, &salt, &crate::ID).unwrap();

        let instruction_data = crate::instruction::FreezeAccount {
            client_id: client_id.clone(),
            sender: sender.clone(),
            salt: salt.clone(),
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new_readonly(app_state_pda, false),
                AccountMeta::new(account_pda, false),
                AccountMeta::new_readonly(wrong_authority, true),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_gmp_app_state_account(
                app_state_pda,
                router_program,
                port_id,
                authority,
                app_state_bump,
                false, // not paused
            ),
            create_account_state_with_nonce(
                account_pda,
                client_id,
                sender,
                salt,
                0,
                false,
                account_bump,
            ),
            create_authority_account(wrong_authority),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "Freeze account should fail with wrong authority"
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
        let port_id = "gmpport".to_string();

        let (app_state_pda, app_state_bump) = Pubkey::find_program_address(
            &[crate::constants::GMP_APP_STATE_SEED, port_id.as_bytes()],
            &crate::ID,
        );

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
                port_id,
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
        let app_state_data = &app_state_account.data[8..];
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
        let port_id = "gmpport".to_string();

        let (app_state_pda, app_state_bump) = Pubkey::find_program_address(
            &[crate::constants::GMP_APP_STATE_SEED, port_id.as_bytes()],
            &crate::ID,
        );

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
                port_id,
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
    fn test_freeze_account_invalid_pda() {
        let mollusk = Mollusk::new(&crate::ID, crate::get_gmp_program_path());

        let authority = Pubkey::new_unique();
        let router_program = Pubkey::new_unique();
        let port_id = "gmpport".to_string();
        let client_id = "cosmoshub-1".to_string();
        let sender = "cosmos1test".to_string();
        let salt = vec![1, 2, 3];

        let (app_state_pda, app_state_bump) = Pubkey::find_program_address(
            &[crate::constants::GMP_APP_STATE_SEED, port_id.as_bytes()],
            &crate::ID,
        );

        let (_correct_account_pda, _correct_bump) =
            AccountState::derive_address(&client_id, &sender, &salt, &crate::ID).unwrap();

        // Use wrong account PDA
        let wrong_account_pda = Pubkey::new_unique();

        let instruction_data = crate::instruction::FreezeAccount {
            client_id: client_id.clone(),
            sender: sender.clone(),
            salt: salt.clone(),
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new_readonly(app_state_pda, false),
                AccountMeta::new(wrong_account_pda, false), // Wrong PDA!
                AccountMeta::new_readonly(authority, true),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_gmp_app_state_account(
                app_state_pda,
                router_program,
                port_id,
                authority,
                app_state_bump,
                false, // not paused
            ),
            // Create account at wrong PDA for testing
            create_account_state_with_nonce(
                wrong_account_pda,
                client_id,
                sender,
                salt,
                0,
                false,
                255u8,
            ),
            create_authority_account(authority),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "FreezeAccount should fail with invalid account_state PDA"
        );
    }

    #[test]
    fn test_pause_app_invalid_pda() {
        let mollusk = Mollusk::new(&crate::ID, crate::get_gmp_program_path());

        let authority = Pubkey::new_unique();
        let router_program = Pubkey::new_unique();
        let port_id = "gmpport".to_string();

        let (_correct_app_state_pda, _correct_bump) = Pubkey::find_program_address(
            &[crate::constants::GMP_APP_STATE_SEED, port_id.as_bytes()],
            &crate::ID,
        );

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
                port_id,
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

    #[test]
    fn test_freeze_account_params_too_long() {
        let mollusk = Mollusk::new(&crate::ID, crate::get_gmp_program_path());

        let authority = Pubkey::new_unique();
        let router_program = Pubkey::new_unique();
        let port_id = "gmpport".to_string();
        let valid_client_id = "cosmoshub-1".to_string();
        let valid_sender = "cosmos1test".to_string();
        let valid_salt = vec![1, 2, 3];

        // Create client_id that exceeds MAX_CLIENT_ID_LENGTH
        let client_id_too_long = "a".repeat(crate::constants::MAX_CLIENT_ID_LENGTH + 1);

        let (app_state_pda, app_state_bump) = Pubkey::find_program_address(
            &[crate::constants::GMP_APP_STATE_SEED, port_id.as_bytes()],
            &crate::ID,
        );

        // Use valid parameters for PDA derivation, but pass too-long client_id to instruction
        let (account_pda, account_bump) =
            AccountState::derive_address(&valid_client_id, &valid_sender, &valid_salt, &crate::ID)
                .unwrap();

        let instruction_data = crate::instruction::FreezeAccount {
            client_id: client_id_too_long, // Too long!
            sender: valid_sender.clone(),
            salt: valid_salt.clone(),
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new_readonly(app_state_pda, false),
                AccountMeta::new(account_pda, false),
                AccountMeta::new_readonly(authority, true),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_gmp_app_state_account(
                app_state_pda,
                router_program,
                port_id,
                authority,
                app_state_bump,
                false, // not paused
            ),
            create_account_state_with_nonce(
                account_pda,
                valid_client_id,
                valid_sender,
                valid_salt,
                0,
                false,
                account_bump,
            ),
            create_authority_account(authority),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "FreezeAccount should fail when client_id exceeds MAX_CLIENT_ID_LENGTH"
        );
    }
}
