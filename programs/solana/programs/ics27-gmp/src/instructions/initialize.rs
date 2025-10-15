use crate::constants::*;
use crate::errors::GMPError;
use crate::events::{GMPAppInitialized, RouterCallerCreated};
use crate::state::GMPAppState;
use anchor_lang::prelude::*;

/// Initialize the ICS27 GMP application
#[derive(Accounts)]
#[instruction(router_program: Pubkey, port_id: String)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = payer,
        space = 8 + GMPAppState::INIT_SPACE,
        seeds = [GMP_APP_STATE_SEED, port_id.as_bytes()],
        bump
    )]
    pub app_state: Account<'info, GMPAppState>,

    /// Router caller PDA that represents our app to the router
    /// CHECK: This is a PDA that just needs to exist for router authorization
    #[account(
        init,
        payer = payer,
        space = 8, // Minimal space for account existence
        seeds = [b"router_caller"],
        bump,
    )]
    pub router_caller: AccountInfo<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

pub fn initialize(ctx: Context<Initialize>, router_program: Pubkey, port_id: String) -> Result<()> {
    let app_state = &mut ctx.accounts.app_state;
    // Get clock directly via syscall
    let clock = Clock::get()?;

    // Validate inputs
    require!(!port_id.is_empty(), GMPError::InvalidPacketData);
    require!(port_id.len() <= MAX_PORT_ID_LENGTH, GMPError::PortIdTooLong);

    // Initialize app state
    app_state.router_program = router_program;
    app_state.port_id.clone_from(&port_id);
    app_state.authority = ctx.accounts.authority.key();
    app_state.version = 1;
    app_state.paused = false;
    app_state.bump = ctx.bumps.app_state;

    // Emit initialization events
    emit!(GMPAppInitialized {
        router_program,
        authority: app_state.authority,
        port_id,
        timestamp: clock.unix_timestamp,
    });

    emit!(RouterCallerCreated {
        router_caller: ctx.accounts.router_caller.key(),
        bump: ctx.bumps.router_caller,
    });

    msg!(
        "ICS27 GMP app initialized with router: {}, router_caller: {}",
        router_program,
        ctx.accounts.router_caller.key()
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
    use solana_sdk::pubkey::Pubkey;
    use solana_sdk::system_program;

    fn create_initialize_instruction(
        app_state: Pubkey,
        router_caller: Pubkey,
        payer: Pubkey,
        authority: Pubkey,
        router_program: Pubkey,
        port_id: String,
    ) -> Instruction {
        let instruction_data = crate::instruction::Initialize {
            router_program,
            port_id,
        };

        Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(app_state, false),
                AccountMeta::new(router_caller, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(authority, true),
                AccountMeta::new_readonly(system_program::ID, false),
            ],
            data: instruction_data.data(),
        }
    }

    #[test]
    fn test_initialize_success() {
        let authority = Pubkey::new_unique();
        let payer = authority;
        let router_program = Pubkey::new_unique();
        let port_id = "gmp".to_string();

        let (app_state_pda, _) =
            Pubkey::find_program_address(&[GMP_APP_STATE_SEED, port_id.as_bytes()], &crate::ID);

        let (router_caller_pda, _) = Pubkey::find_program_address(&[b"router_caller"], &crate::ID);

        let instruction = create_initialize_instruction(
            app_state_pda,
            router_caller_pda,
            payer,
            authority,
            router_program,
            port_id,
        );

        let accounts = vec![
            (app_state_pda, solana_sdk::account::Account::default()),
            (router_caller_pda, solana_sdk::account::Account::default()),
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
    fn test_initialize_port_id_too_long() {
        let authority = Pubkey::new_unique();
        let payer = authority;
        let router_program = Pubkey::new_unique();
        let port_id_too_long = "a".repeat(MAX_PORT_ID_LENGTH + 1);

        // Use a valid short port_id for PDA derivation, but pass the too-long one to instruction
        let valid_port_id = "gmp";
        let (app_state_pda, _) = Pubkey::find_program_address(
            &[GMP_APP_STATE_SEED, valid_port_id.as_bytes()],
            &crate::ID,
        );

        let (router_caller_pda, _) = Pubkey::find_program_address(&[b"router_caller"], &crate::ID);

        let instruction = create_initialize_instruction(
            app_state_pda,
            router_caller_pda,
            payer,
            authority,
            router_program,
            port_id_too_long,
        );

        let accounts = vec![
            (app_state_pda, solana_sdk::account::Account::default()),
            (router_caller_pda, solana_sdk::account::Account::default()),
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

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(result.program_result.is_err());
    }

    #[test]
    fn test_initialize_empty_port_id() {
        let authority = Pubkey::new_unique();
        let payer = authority;
        let router_program = Pubkey::new_unique();
        let empty_port_id = String::new();

        let valid_port_id = "gmp";
        let (app_state_pda, _) = Pubkey::find_program_address(
            &[GMP_APP_STATE_SEED, valid_port_id.as_bytes()],
            &crate::ID,
        );

        let (router_caller_pda, _) = Pubkey::find_program_address(&[b"router_caller"], &crate::ID);

        let instruction = create_initialize_instruction(
            app_state_pda,
            router_caller_pda,
            payer,
            authority,
            router_program,
            empty_port_id,
        );

        let accounts = vec![
            (app_state_pda, solana_sdk::account::Account::default()),
            (router_caller_pda, solana_sdk::account::Account::default()),
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

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "Initialize should fail with empty port_id"
        );
    }

    #[test]
    fn test_initialize_already_initialized() {
        let authority = Pubkey::new_unique();
        let payer = authority;
        let router_program = Pubkey::new_unique();
        let port_id = "gmp".to_string();

        let (app_state_pda, _) =
            Pubkey::find_program_address(&[GMP_APP_STATE_SEED, port_id.as_bytes()], &crate::ID);

        let (router_caller_pda, _) = Pubkey::find_program_address(&[b"router_caller"], &crate::ID);

        let instruction = create_initialize_instruction(
            app_state_pda,
            router_caller_pda,
            payer,
            authority,
            router_program,
            port_id,
        );

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
            (router_caller_pda, solana_sdk::account::Account::default()),
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

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(
            result.program_result.is_err(),
            "Initialize should fail when account already initialized"
        );
    }
}
