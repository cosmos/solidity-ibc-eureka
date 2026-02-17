use crate::constants::*;
use crate::events::GMPAppInitialized;
use crate::state::{AccountVersion, GMPAppState};
use anchor_lang::prelude::*;

/// Initialize the ICS27 GMP application
#[derive(Accounts)]
#[instruction(access_manager: Pubkey)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = payer,
        space = 8 + GMPAppState::INIT_SPACE,
        seeds = [GMPAppState::SEED],
        bump
    )]
    pub app_state: Account<'info, GMPAppState>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn initialize(ctx: Context<Initialize>, access_manager: Pubkey) -> Result<()> {
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
            ],
            data: instruction_data.data(),
        }
    }

    #[test]
    fn test_initialize_success() {
        let payer = Pubkey::new_unique();

        let (app_state_pda, _) = Pubkey::find_program_address(&[GMPAppState::SEED], &crate::ID);

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
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::get_gmp_program_path());

        let checks = vec![
            Check::success(),
            Check::account(&app_state_pda).owner(&crate::ID).build(),
        ];

        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }

    #[test]
    fn test_initialize_cannot_reinitialize() {
        let payer = Pubkey::new_unique();

        let (app_state_pda, _) = Pubkey::find_program_address(&[GMPAppState::SEED], &crate::ID);

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
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::get_gmp_program_path());

        // Anchor's `init` constraint fails when account already exists
        // Error code 0 means the account is already in use
        let checks = vec![Check::err(ProgramError::Custom(0))];

        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }
}
