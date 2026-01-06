use crate::errors::RouterError;
use crate::state::{IBCApp, RouterState, MAX_UPSTREAM_CALLERS};
use anchor_lang::prelude::*;

#[derive(Accounts)]
#[instruction(port_id: String)]
pub struct AddUpstreamCaller<'info> {
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
        mut,
        seeds = [IBCApp::SEED, port_id.as_bytes()],
        bump
    )]
    pub ibc_app: Account<'info, IBCApp>,

    pub authority: Signer<'info>,

    /// Instructions sysvar for CPI validation
    /// CHECK: Address constraint verifies this is the instructions sysvar
    #[account(address = anchor_lang::solana_program::sysvar::instructions::ID)]
    pub instructions_sysvar: AccountInfo<'info>,
}

pub fn add_upstream_caller(
    ctx: Context<AddUpstreamCaller>,
    _port_id: String,
    upstream_caller: Pubkey,
) -> Result<()> {
    // Verify caller has ID_CUSTOMIZER_ROLE (same as add_ibc_app)
    access_manager::require_role(
        &ctx.accounts.access_manager,
        solana_ibc_types::roles::ID_CUSTOMIZER_ROLE,
        &ctx.accounts.authority,
        &ctx.accounts.instructions_sysvar,
        &crate::ID,
    )?;

    let ibc_app = &mut ctx.accounts.ibc_app;

    // Check we haven't reached the maximum number of upstream callers
    require!(
        ibc_app.upstream_callers.len() < MAX_UPSTREAM_CALLERS,
        RouterError::TooManyUpstreamCallers
    );

    // Check the caller isn't already registered
    require!(
        !ibc_app.upstream_callers.contains(&upstream_caller),
        RouterError::UpstreamCallerAlreadyExists
    );

    ibc_app.upstream_callers.push(upstream_caller);

    msg!(
        "Added upstream caller {} for port {}",
        upstream_caller,
        ibc_app.port_id
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
    use solana_ibc_types::roles;
    use solana_sdk::instruction::{AccountMeta, Instruction};
    use solana_sdk::program_error::ProgramError;
    use solana_sdk::pubkey::Pubkey;

    #[test]
    fn test_add_upstream_caller_happy_path() {
        let authority = Pubkey::new_unique();
        let port_id = "test-port";
        let app_program = Pubkey::new_unique();
        let upstream_caller = Pubkey::new_unique();

        let (router_state_pda, router_state_data) = setup_router_state();
        let (access_manager_pda, access_manager_data) =
            setup_access_manager_with_roles(&[(roles::ID_CUSTOMIZER_ROLE, &[authority])]);
        let (ibc_app_pda, ibc_app_data) = setup_ibc_app(port_id, app_program);

        let instruction_data = crate::instruction::AddUpstreamCaller {
            port_id: port_id.to_string(),
            upstream_caller,
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new_readonly(router_state_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new(ibc_app_pda, false),
                AccountMeta::new_readonly(authority, true),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_account(router_state_pda, router_state_data, crate::ID),
            create_account(access_manager_pda, access_manager_data, access_manager::ID),
            create_account(ibc_app_pda, ibc_app_data, crate::ID),
            create_system_account(authority),
            create_instructions_sysvar_account_with_caller(crate::ID),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::test_utils::get_router_program_path());

        let checks = vec![Check::success()];

        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }

    #[test]
    fn test_add_upstream_caller_unauthorized() {
        let authorized_user = Pubkey::new_unique();
        let unauthorized_user = Pubkey::new_unique();
        let port_id = "test-port";
        let app_program = Pubkey::new_unique();
        let upstream_caller = Pubkey::new_unique();

        let (router_state_pda, router_state_data) = setup_router_state();
        let (access_manager_pda, access_manager_data) =
            setup_access_manager_with_roles(&[(roles::ID_CUSTOMIZER_ROLE, &[authorized_user])]);
        let (ibc_app_pda, ibc_app_data) = setup_ibc_app(port_id, app_program);

        let instruction_data = crate::instruction::AddUpstreamCaller {
            port_id: port_id.to_string(),
            upstream_caller,
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new_readonly(router_state_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new(ibc_app_pda, false),
                AccountMeta::new_readonly(unauthorized_user, true),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_account(router_state_pda, router_state_data, crate::ID),
            create_account(access_manager_pda, access_manager_data, access_manager::ID),
            create_account(ibc_app_pda, ibc_app_data, crate::ID),
            create_system_account(unauthorized_user),
            create_instructions_sysvar_account_with_caller(crate::ID),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::test_utils::get_router_program_path());

        let checks = vec![Check::err(ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + RouterError::UnauthorizedSender as u32,
        ))];

        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }

    #[test]
    fn test_add_upstream_caller_already_exists() {
        let authority = Pubkey::new_unique();
        let port_id = "test-port";
        let app_program = Pubkey::new_unique();
        let upstream_caller = Pubkey::new_unique();

        let (router_state_pda, router_state_data) = setup_router_state();
        let (access_manager_pda, access_manager_data) =
            setup_access_manager_with_roles(&[(roles::ID_CUSTOMIZER_ROLE, &[authority])]);
        // Setup IBCApp with the upstream_caller already registered
        let (ibc_app_pda, ibc_app_data) =
            setup_ibc_app_with_upstream(port_id, app_program, vec![upstream_caller]);

        let instruction_data = crate::instruction::AddUpstreamCaller {
            port_id: port_id.to_string(),
            upstream_caller,
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new_readonly(router_state_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new(ibc_app_pda, false),
                AccountMeta::new_readonly(authority, true),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_account(router_state_pda, router_state_data, crate::ID),
            create_account(access_manager_pda, access_manager_data, access_manager::ID),
            create_account(ibc_app_pda, ibc_app_data, crate::ID),
            create_system_account(authority),
            create_instructions_sysvar_account_with_caller(crate::ID),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::test_utils::get_router_program_path());

        let checks = vec![Check::err(ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + RouterError::UpstreamCallerAlreadyExists as u32,
        ))];

        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }

    #[test]
    fn test_add_upstream_caller_too_many() {
        let authority = Pubkey::new_unique();
        let port_id = "test-port";
        let app_program = Pubkey::new_unique();

        // Create MAX_UPSTREAM_CALLERS already registered
        let existing_callers: Vec<Pubkey> = (0..MAX_UPSTREAM_CALLERS)
            .map(|_| Pubkey::new_unique())
            .collect();
        let new_upstream_caller = Pubkey::new_unique();

        let (router_state_pda, router_state_data) = setup_router_state();
        let (access_manager_pda, access_manager_data) =
            setup_access_manager_with_roles(&[(roles::ID_CUSTOMIZER_ROLE, &[authority])]);
        let (ibc_app_pda, ibc_app_data) =
            setup_ibc_app_with_upstream(port_id, app_program, existing_callers);

        let instruction_data = crate::instruction::AddUpstreamCaller {
            port_id: port_id.to_string(),
            upstream_caller: new_upstream_caller,
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new_readonly(router_state_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new(ibc_app_pda, false),
                AccountMeta::new_readonly(authority, true),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_account(router_state_pda, router_state_data, crate::ID),
            create_account(access_manager_pda, access_manager_data, access_manager::ID),
            create_account(ibc_app_pda, ibc_app_data, crate::ID),
            create_system_account(authority),
            create_instructions_sysvar_account_with_caller(crate::ID),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::test_utils::get_router_program_path());

        let checks = vec![Check::err(ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + RouterError::TooManyUpstreamCallers as u32,
        ))];

        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }
}
