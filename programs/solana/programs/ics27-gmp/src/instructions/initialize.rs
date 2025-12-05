use crate::constants::*;
use crate::errors::GMPError;
use crate::events::GMPAppInitialized;
use crate::state::{AccountVersion, GMPAppState};
use anchor_lang::prelude::*;
use solana_ibc_types::cpi::reject_cpi;

/// Initialize the ICS27 GMP application
#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = payer,
        space = 8 + GMPAppState::INIT_SPACE,
        seeds = [GMPAppState::SEED, GMP_PORT_ID.as_bytes()],
        bump
    )]
    pub app_state: Account<'info, GMPAppState>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,

    /// Instructions sysvar for CPI validation
    /// CHECK: Address constraint verifies this is the instructions sysvar
    #[account(address = anchor_lang::solana_program::sysvar::instructions::ID)]
    pub instructions_sysvar: AccountInfo<'info>,
}

pub fn initialize(ctx: Context<Initialize>, access_manager: Pubkey) -> Result<()> {
    // Reject CPI calls - this instruction must be called directly
    reject_cpi(&ctx.accounts.instructions_sysvar, &crate::ID).map_err(GMPError::from)?;

    let app_state = &mut ctx.accounts.app_state;
    let clock = Clock::get()?;

    // Initialize app state
    app_state.version = AccountVersion::V1;
    app_state.paused = false;
    app_state.bump = ctx.bumps.app_state;
    app_state.access_manager = access_manager;
    app_state._reserved = [0; 256];

    // Emit initialization event
    emit!(GMPAppInitialized {
        router_program: ics26_router::ID,
        port_id: GMP_PORT_ID.to_string(),
        timestamp: clock.unix_timestamp,
    });

    msg!(
        "ICS27 GMP app initialized with router: {}, port_id: {}",
        ics26_router::ID,
        GMP_PORT_ID
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::*;
    use anchor_lang::InstructionData;
    use mollusk_svm::result::Check;
    use mollusk_svm::Mollusk;
    use solana_sdk::instruction::{AccountMeta, Instruction};
    use solana_sdk::program_error::ProgramError;
    use solana_sdk::pubkey::Pubkey;
    use solana_sdk::system_program;

    fn create_initialize_instruction(app_state: Pubkey, payer: Pubkey) -> Instruction {
        let instruction_data = crate::instruction::Initialize {
            access_manager: access_manager::ID,
        };

        Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(app_state, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(system_program::ID, false),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
            data: instruction_data.data(),
        }
    }

    #[test]
    fn test_initialize_success() {
        let payer = Pubkey::new_unique();

        let (app_state_pda, _) =
            Pubkey::find_program_address(&[GMPAppState::SEED, GMP_PORT_ID.as_bytes()], &crate::ID);

        let instruction = create_initialize_instruction(app_state_pda, payer);

        let accounts = vec![
            (app_state_pda, solana_sdk::account::Account::default()),
            (
                payer,
                solana_sdk::account::Account {
                    lamports: 1_000_000_000,
                    owner: system_program::ID,
                    ..Default::default()
                },
            ),
            (
                system_program::ID,
                solana_sdk::account::Account {
                    lamports: 1,
                    executable: true,
                    owner: solana_sdk::native_loader::ID,
                    ..Default::default()
                },
            ),
            create_instructions_sysvar_account_with_caller(crate::ID),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::get_gmp_program_path());

        let checks = vec![
            Check::success(),
            Check::account(&app_state_pda).owner(&crate::ID).build(),
        ];

        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }

    #[test]
    fn test_initialize_already_initialized() {
        let payer = Pubkey::new_unique();

        let (app_state_pda, _) =
            Pubkey::find_program_address(&[GMPAppState::SEED, GMP_PORT_ID.as_bytes()], &crate::ID);

        let instruction = create_initialize_instruction(app_state_pda, payer);

        // Create accounts that are already initialized (owned by program, not system)
        let accounts = vec![
            (
                app_state_pda,
                solana_sdk::account::Account {
                    lamports: 1_000_000,
                    data: vec![0; 100], // Already has data
                    owner: crate::ID,   // Already owned by program
                    ..Default::default()
                },
            ),
            (
                payer,
                solana_sdk::account::Account {
                    lamports: 1_000_000_000,
                    owner: system_program::ID,
                    ..Default::default()
                },
            ),
            (
                system_program::ID,
                solana_sdk::account::Account {
                    lamports: 1,
                    executable: true,
                    owner: solana_sdk::native_loader::ID,
                    ..Default::default()
                },
            ),
            create_instructions_sysvar_account_with_caller(crate::ID),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::get_gmp_program_path());

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "Initialize should fail when account already initialized"
        );
    }

    #[test]
    fn test_initialize_fake_sysvar_wormhole_attack() {
        let payer = Pubkey::new_unique();

        let (app_state_pda, _) =
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

        // Replace real sysvar with fake one (Wormhole-style attack)
        let (instruction, fake_sysvar_account) = setup_fake_sysvar_attack(instruction, crate::ID);

        let accounts = vec![
            (app_state_pda, solana_sdk::account::Account::default()),
            (
                payer,
                solana_sdk::account::Account {
                    lamports: 1_000_000_000,
                    owner: system_program::ID,
                    ..Default::default()
                },
            ),
            (
                system_program::ID,
                solana_sdk::account::Account {
                    lamports: 1,
                    executable: true,
                    owner: solana_sdk::native_loader::ID,
                    ..Default::default()
                },
            ),
            fake_sysvar_account,
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::get_gmp_program_path());

        mollusk.process_and_validate_instruction(
            &instruction,
            &accounts,
            &[expect_sysvar_attack_error()],
        );
    }

    #[test]
    fn test_initialize_cpi_rejection() {
        let payer = Pubkey::new_unique();

        let (app_state_pda, _) =
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

        // Simulate CPI call from unauthorized program
        let malicious_program = Pubkey::new_unique();
        let (instruction, cpi_sysvar_account) = setup_cpi_call_test(instruction, malicious_program);

        let accounts = vec![
            (app_state_pda, solana_sdk::account::Account::default()),
            (
                payer,
                solana_sdk::account::Account {
                    lamports: 1_000_000_000,
                    owner: system_program::ID,
                    ..Default::default()
                },
            ),
            (
                system_program::ID,
                solana_sdk::account::Account {
                    lamports: 1,
                    executable: true,
                    owner: solana_sdk::native_loader::ID,
                    ..Default::default()
                },
            ),
            cpi_sysvar_account,
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::get_gmp_program_path());

        let checks = vec![Check::err(ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + crate::errors::GMPError::UnauthorizedRouter as u32,
        ))];
        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }
}
