use crate::errors::RouterError;
use crate::state::{IBCApp, RouterState, IBC_APP_SEED, ROUTER_STATE_SEED};
use anchor_lang::prelude::*;
use solana_ibc_types::events::IBCAppAdded;

#[derive(Accounts)]
#[instruction(port_id: String)]
pub struct AddIbcApp<'info> {
    #[account(
        seeds = [ROUTER_STATE_SEED],
        bump
    )]
    pub router_state: Account<'info, RouterState>,

    #[account(
        init,
        payer = payer,
        space = 8 + IBCApp::INIT_SPACE,
        seeds = [IBC_APP_SEED, port_id.as_bytes()],
        bump
    )]
    pub ibc_app: Account<'info, IBCApp>,

    /// The IBC application program to register
    /// CHECK: This is the program ID of the IBC app
    pub app_program: UncheckedAccount<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub authority: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn add_ibc_app(ctx: Context<AddIbcApp>, port_id: String) -> Result<()> {
    let router_state = &ctx.accounts.router_state;
    let ibc_app = &mut ctx.accounts.ibc_app;

    require!(
        ctx.accounts.authority.key() == router_state.authority,
        RouterError::UnauthorizedSender
    );

    require!(!port_id.is_empty(), RouterError::InvalidPortIdentifier);

    ibc_app.port_id = port_id;
    ibc_app.app_program_id = ctx.accounts.app_program.key();
    ibc_app.authority = ctx.accounts.authority.key();

    emit!(IBCAppAdded {
        port_id: ibc_app.port_id.clone(),
        app_program_id: ibc_app.app_program_id,
    });

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

    #[test]
    fn test_add_ibc_app_happy_path() {
        let authority = Pubkey::new_unique();
        let payer = authority;
        let port_id = "test-port";
        let app_program = Pubkey::new_unique();

        let (router_state_pda, router_state_data) = setup_router_state(authority);

        let (ibc_app_pda, _) =
            Pubkey::find_program_address(&[IBC_APP_SEED, port_id.as_bytes()], &crate::ID);

        let instruction_data = crate::instruction::AddIbcApp {
            port_id: port_id.to_string(),
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new_readonly(router_state_pda, false),
                AccountMeta::new(ibc_app_pda, false),
                AccountMeta::new_readonly(app_program, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(authority, true),
                AccountMeta::new_readonly(system_program::ID, false),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_account(router_state_pda, router_state_data, crate::ID),
            create_uninitialized_account(ibc_app_pda, 0),
            create_account(app_program, vec![], system_program::ID),
            create_system_account(payer),
            create_program_account(system_program::ID),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());

        let checks = vec![
            Check::success(),
            Check::account(&ibc_app_pda).owner(&crate::ID).build(),
        ];

        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }

    #[test]
    fn test_add_ibc_app_unauthorized_sender() {
        let authority = Pubkey::new_unique();
        let unauthorized_sender = Pubkey::new_unique();
        let payer = unauthorized_sender;
        let port_id = "test-port";
        let app_program = Pubkey::new_unique();

        let (router_state_pda, router_state_data) = setup_router_state(authority);

        let (ibc_app_pda, _) =
            Pubkey::find_program_address(&[IBC_APP_SEED, port_id.as_bytes()], &crate::ID);

        let instruction_data = crate::instruction::AddIbcApp {
            port_id: port_id.to_string(),
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new_readonly(router_state_pda, false),
                AccountMeta::new(ibc_app_pda, false),
                AccountMeta::new_readonly(app_program, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(unauthorized_sender, true),
                AccountMeta::new_readonly(system_program::ID, false),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_account(router_state_pda, router_state_data, crate::ID),
            create_uninitialized_account(ibc_app_pda, 0),
            create_account(app_program, vec![], system_program::ID),
            create_system_account(payer),
            create_program_account(system_program::ID),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());

        let checks = vec![Check::err(ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + RouterError::UnauthorizedSender as u32,
        ))];

        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }

    #[test]
    fn test_add_ibc_app_invalid_port_identifier() {
        let authority = Pubkey::new_unique();
        let payer = authority;
        let port_id = ""; // Empty port ID
        let app_program = Pubkey::new_unique();

        let (router_state_pda, router_state_data) = setup_router_state(authority);

        let (ibc_app_pda, _) =
            Pubkey::find_program_address(&[IBC_APP_SEED, port_id.as_bytes()], &crate::ID);

        let instruction_data = crate::instruction::AddIbcApp {
            port_id: port_id.to_string(),
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new_readonly(router_state_pda, false),
                AccountMeta::new(ibc_app_pda, false),
                AccountMeta::new_readonly(app_program, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(authority, true),
                AccountMeta::new_readonly(system_program::ID, false),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_account(router_state_pda, router_state_data, crate::ID),
            create_uninitialized_account(ibc_app_pda, 0),
            create_account(app_program, vec![], system_program::ID),
            create_system_account(payer),
            create_program_account(system_program::ID),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());

        let checks = vec![Check::err(ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + RouterError::InvalidPortIdentifier as u32,
        ))];

        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }

    #[test]
    fn test_add_ibc_app_already_exists() {
        let authority = Pubkey::new_unique();
        let payer = authority;
        let port_id = "test-port";
        let app_program = Pubkey::new_unique();

        let (router_state_pda, router_state_data) = setup_router_state(authority);

        // IBC app already exists
        let (ibc_app_pda, existing_ibc_app_data) = setup_ibc_app(port_id, Pubkey::new_unique());

        let instruction_data = crate::instruction::AddIbcApp {
            port_id: port_id.to_string(),
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new_readonly(router_state_pda, false),
                AccountMeta::new(ibc_app_pda, false),
                AccountMeta::new_readonly(app_program, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(authority, true),
                AccountMeta::new_readonly(system_program::ID, false),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_account(router_state_pda, router_state_data, crate::ID),
            create_account(ibc_app_pda, existing_ibc_app_data, crate::ID),
            create_account(app_program, vec![], system_program::ID),
            create_system_account(payer),
            create_program_account(system_program::ID),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());

        // This should fail because the account already exists (init constraint violation)
        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(result.program_result.is_err());
    }
}
