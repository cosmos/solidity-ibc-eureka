use crate::errors::RouterError;
use crate::state::{AccountVersion, IBCApp, RouterState};
use crate::IBCAppAdded;
use anchor_lang::prelude::*;

#[derive(Accounts)]
#[instruction(port_id: String)]
pub struct AddIbcApp<'info> {
    #[account(
        seeds = [RouterState::SEED],
        bump
    )]
    pub router_state: Account<'info, RouterState>,

    /// CHECK: Validated via seeds constraint using stored `access_manager` program ID
    #[account(
        seeds = [access_manager::state::AccessManager::SEED],
        bump,
        seeds::program = router_state.access_manager
    )]
    pub access_manager: AccountInfo<'info>,

    #[account(
        init,
        payer = payer,
        space = 8 + IBCApp::INIT_SPACE,
        seeds = [IBCApp::SEED, port_id.as_bytes()],
        bump
    )]
    pub ibc_app: Account<'info, IBCApp>,

    /// The IBC application program to register
    /// CHECK: Unchecked because only used to extract program ID
    pub app_program: UncheckedAccount<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub authority: Signer<'info>,

    pub system_program: Program<'info, System>,

    /// Instructions sysvar for CPI validation
    /// CHECK: Address constraint verifies this is the instructions sysvar
    #[account(address = anchor_lang::solana_program::sysvar::instructions::ID)]
    pub instructions_sysvar: AccountInfo<'info>,
}

pub fn add_ibc_app(ctx: Context<AddIbcApp>, port_id: String) -> Result<()> {
    access_manager::require_role(
        &ctx.accounts.access_manager,
        solana_ibc_types::roles::ID_CUSTOMIZER_ROLE,
        &ctx.accounts.authority,
        &ctx.accounts.instructions_sysvar,
        &crate::ID,
    )?;

    require!(!port_id.is_empty(), RouterError::InvalidPortIdentifier);

    let ibc_app = &mut ctx.accounts.ibc_app;
    ibc_app.version = AccountVersion::V1;
    ibc_app.port_id = port_id;
    ibc_app.app_program_id = ctx.accounts.app_program.key();
    ibc_app.authority = ctx.accounts.authority.key();
    ibc_app._reserved = [0u8; 256];

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
    use solana_ibc_types::roles;
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

        let (router_state_pda, router_state_data) = setup_router_state();

        let (access_manager_pda, access_manager_data) =
            setup_access_manager_with_roles(&[(roles::ID_CUSTOMIZER_ROLE, &[authority])]);

        let (ibc_app_pda, _) =
            Pubkey::find_program_address(&[IBCApp::SEED, port_id.as_bytes()], &crate::ID);

        let instruction_data = crate::instruction::AddIbcApp {
            port_id: port_id.to_string(),
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new_readonly(router_state_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new(ibc_app_pda, false),
                AccountMeta::new_readonly(app_program, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(authority, true),
                AccountMeta::new_readonly(system_program::ID, false),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_account(router_state_pda, router_state_data, crate::ID),
            create_account(access_manager_pda, access_manager_data, access_manager::ID),
            create_uninitialized_account(ibc_app_pda, 0),
            create_account(app_program, vec![], system_program::ID),
            create_system_account(payer),
            create_system_account(authority),
            create_program_account(system_program::ID),
            create_instructions_sysvar_account_with_caller(crate::ID),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::test_utils::get_router_program_path());

        let checks = vec![
            Check::success(),
            Check::account(&ibc_app_pda).owner(&crate::ID).build(),
        ];

        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }

    #[test]
    fn test_add_ibc_app_unauthorized_sender() {
        let authorized_user = Pubkey::new_unique();
        let unauthorized_sender = Pubkey::new_unique();
        let payer = unauthorized_sender;
        let port_id = "test-port";
        let app_program = Pubkey::new_unique();

        let (router_state_pda, router_state_data) = setup_router_state();

        // Setup access manager with authorized_user having ID_CUSTOMIZER_ROLE, but NOT unauthorized_sender
        let (access_manager_pda, access_manager_data) =
            setup_access_manager_with_roles(&[(roles::ID_CUSTOMIZER_ROLE, &[authorized_user])]);

        let (ibc_app_pda, _) =
            Pubkey::find_program_address(&[IBCApp::SEED, port_id.as_bytes()], &crate::ID);

        let instruction_data = crate::instruction::AddIbcApp {
            port_id: port_id.to_string(),
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new_readonly(router_state_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new(ibc_app_pda, false),
                AccountMeta::new_readonly(app_program, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(unauthorized_sender, true),
                AccountMeta::new_readonly(system_program::ID, false),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_account(router_state_pda, router_state_data, crate::ID),
            create_account(access_manager_pda, access_manager_data, access_manager::ID),
            create_uninitialized_account(ibc_app_pda, 0),
            create_account(app_program, vec![], system_program::ID),
            create_system_account(payer),
            create_system_account(unauthorized_sender),
            create_program_account(system_program::ID),
            create_instructions_sysvar_account_with_caller(crate::ID),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::test_utils::get_router_program_path());

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

        let (router_state_pda, router_state_data) = setup_router_state();
        let (access_manager_pda, access_manager_data) =
            setup_access_manager_with_roles(&[(roles::ID_CUSTOMIZER_ROLE, &[authority])]);
        let (ibc_app_pda, _) =
            Pubkey::find_program_address(&[IBCApp::SEED, port_id.as_bytes()], &crate::ID);

        let instruction_data = crate::instruction::AddIbcApp {
            port_id: port_id.to_string(),
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new_readonly(router_state_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new(ibc_app_pda, false),
                AccountMeta::new_readonly(app_program, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(authority, true),
                AccountMeta::new_readonly(system_program::ID, false),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_account(router_state_pda, router_state_data, crate::ID),
            create_account(access_manager_pda, access_manager_data, access_manager::ID),
            create_uninitialized_account(ibc_app_pda, 0),
            create_account(app_program, vec![], system_program::ID),
            create_system_account(payer),
            create_system_account(authority),
            create_program_account(system_program::ID),
            create_instructions_sysvar_account_with_caller(crate::ID),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::test_utils::get_router_program_path());

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

        let (router_state_pda, router_state_data) = setup_router_state();
        let (access_manager_pda, access_manager_data) =
            setup_access_manager_with_roles(&[(roles::ID_CUSTOMIZER_ROLE, &[authority])]);
        // IBC app already exists
        let (ibc_app_pda, existing_ibc_app_data) = setup_ibc_app(port_id, Pubkey::new_unique());

        let instruction_data = crate::instruction::AddIbcApp {
            port_id: port_id.to_string(),
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new_readonly(router_state_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new(ibc_app_pda, false),
                AccountMeta::new_readonly(app_program, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(authority, true),
                AccountMeta::new_readonly(system_program::ID, false),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_account(router_state_pda, router_state_data, crate::ID),
            create_account(access_manager_pda, access_manager_data, access_manager::ID),
            create_account(ibc_app_pda, existing_ibc_app_data, crate::ID),
            create_account(app_program, vec![], system_program::ID),
            create_system_account(payer),
            create_system_account(authority),
            create_program_account(system_program::ID),
            create_instructions_sysvar_account_with_caller(crate::ID),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::test_utils::get_router_program_path());

        // This should fail because the account already exists (init constraint violation)
        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(result.program_result.is_err());
    }

    #[test]
    fn test_add_ibc_app_fake_sysvar_wormhole_attack() {
        let authority = Pubkey::new_unique();
        let payer = authority;
        let port_id = "test-port";
        let app_program = Pubkey::new_unique();

        let (router_state_pda, router_state_data) = setup_router_state();

        let (access_manager_pda, access_manager_data) =
            setup_access_manager_with_roles(&[(roles::ID_CUSTOMIZER_ROLE, &[authority])]);

        let (ibc_app_pda, _) =
            Pubkey::find_program_address(&[IBCApp::SEED, port_id.as_bytes()], &crate::ID);

        let instruction_data = crate::instruction::AddIbcApp {
            port_id: port_id.to_string(),
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new_readonly(router_state_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new(ibc_app_pda, false),
                AccountMeta::new_readonly(app_program, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(authority, true),
                AccountMeta::new_readonly(system_program::ID, false),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
            data: instruction_data.data(),
        };

        // Replace real sysvar with fake one (Wormhole-style attack)
        let (instruction, fake_sysvar_account) = setup_fake_sysvar_attack(instruction, crate::ID);

        let accounts = vec![
            create_account(router_state_pda, router_state_data, crate::ID),
            create_account(access_manager_pda, access_manager_data, access_manager::ID),
            create_uninitialized_account(ibc_app_pda, 0),
            create_account(app_program, vec![], system_program::ID),
            create_system_account(payer),
            create_program_account(system_program::ID),
            fake_sysvar_account,
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::test_utils::get_router_program_path());

        mollusk.process_and_validate_instruction(
            &instruction,
            &accounts,
            &[expect_sysvar_attack_error()],
        );
    }

    #[test]
    fn test_add_ibc_app_cpi_rejection() {
        let authority = Pubkey::new_unique();
        let payer = authority;
        let port_id = "test-port";
        let app_program = Pubkey::new_unique();

        let (router_state_pda, router_state_data) = setup_router_state();

        let (access_manager_pda, access_manager_data) =
            setup_access_manager_with_roles(&[(roles::ID_CUSTOMIZER_ROLE, &[authority])]);

        let (ibc_app_pda, _) =
            Pubkey::find_program_address(&[IBCApp::SEED, port_id.as_bytes()], &crate::ID);

        let instruction_data = crate::instruction::AddIbcApp {
            port_id: port_id.to_string(),
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new_readonly(router_state_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new(ibc_app_pda, false),
                AccountMeta::new_readonly(app_program, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(authority, true),
                AccountMeta::new_readonly(system_program::ID, false),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
            data: instruction_data.data(),
        };

        // Simulate CPI call from unauthorized program
        let malicious_program = Pubkey::new_unique();
        let (instruction, cpi_sysvar_account) = setup_cpi_call_test(instruction, malicious_program);

        let accounts = vec![
            create_account(router_state_pda, router_state_data, crate::ID),
            create_account(access_manager_pda, access_manager_data, access_manager::ID),
            create_uninitialized_account(ibc_app_pda, 0),
            create_account(app_program, vec![], system_program::ID),
            create_system_account(payer),
            create_system_account(authority),
            create_program_account(system_program::ID),
            cpi_sysvar_account,
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::test_utils::get_router_program_path());

        // When CPI is detected by access_manager::require_role, it returns AccessManagerError::CpiNotAllowed (6005)
        let checks = vec![Check::err(ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + access_manager::AccessManagerError::CpiNotAllowed as u32,
        ))];
        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }
}
