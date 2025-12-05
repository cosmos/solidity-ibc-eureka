use crate::errors::RouterError;
use crate::state::{AccountVersion, RouterState};
use anchor_lang::prelude::*;
use solana_ibc_types::cpi::reject_cpi;

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = payer,
        space = 8 + RouterState::INIT_SPACE,
        seeds = [RouterState::SEED],
        bump
    )]
    pub router_state: Account<'info, RouterState>,

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
    reject_cpi(&ctx.accounts.instructions_sysvar, &crate::ID).map_err(RouterError::from)?;

    let router_state = &mut ctx.accounts.router_state;
    router_state.version = AccountVersion::V1;
    router_state.access_manager = access_manager;
    router_state._reserved = [0u8; 256];
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::RouterError;
    use crate::test_utils::*;
    use anchor_lang::InstructionData;
    use mollusk_svm::result::Check;
    use mollusk_svm::Mollusk;
    use solana_sdk::account::Account;
    use solana_sdk::instruction::{AccountMeta, Instruction};
    use solana_sdk::program_error::ProgramError;
    use solana_sdk::pubkey::Pubkey;
    use solana_sdk::{native_loader, system_program};

    const ANCHOR_ERROR_OFFSET: u32 = 6000;

    #[test]
    fn test_initialize_happy_path() {
        let payer = Pubkey::new_unique();

        let (router_state_pda, _) = Pubkey::find_program_address(&[RouterState::SEED], &crate::ID);

        let instruction_data = crate::instruction::Initialize {
            access_manager: access_manager::ID,
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(router_state_pda, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(system_program::ID, false),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
            data: instruction_data.data(),
        };

        let payer_lamports = 10_000_000_000;
        let accounts = vec![
            (
                router_state_pda,
                Account {
                    lamports: 0,
                    data: vec![],
                    owner: system_program::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            (
                payer,
                Account {
                    lamports: payer_lamports,
                    data: vec![],
                    owner: system_program::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            (
                system_program::ID,
                Account {
                    lamports: 0,
                    data: vec![],
                    owner: native_loader::ID,
                    executable: true,
                    rent_epoch: 0,
                },
            ),
            crate::test_utils::create_instructions_sysvar_account_with_caller(crate::ID),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::test_utils::get_router_program_path());

        let checks = vec![
            Check::success(),
            Check::account(&router_state_pda).owner(&crate::ID).build(),
        ];

        let result = mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);

        let payer_account = result
            .resulting_accounts
            .iter()
            .find(|(pubkey, _)| pubkey == &payer)
            .map(|(_, account)| account)
            .expect("Payer account not found");

        assert!(
            payer_account.lamports < payer_lamports,
            "Payer should have paid for account creation"
        );

        let router_state_account = result
            .resulting_accounts
            .iter()
            .find(|(pubkey, _)| pubkey == &router_state_pda)
            .map(|(_, account)| account)
            .expect("Router state account not found");

        assert!(
            router_state_account.lamports > 0,
            "Router state account should be rent-exempt"
        );
        assert!(
            router_state_account.data.len() > 8,
            "Router state account should have data"
        );

        let deserialized_router_state: RouterState =
            RouterState::try_deserialize(&mut &router_state_account.data[..])
                .expect("Failed to deserialize router state");

        assert_eq!(deserialized_router_state.version, AccountVersion::V1);
        assert_eq!(deserialized_router_state.access_manager, access_manager::ID);
    }

    #[test]
    fn test_initialize_fake_sysvar_wormhole_attack() {
        let payer = Pubkey::new_unique();

        let (router_state_pda, _) = Pubkey::find_program_address(&[RouterState::SEED], &crate::ID);

        // Simulate Wormhole attack: pass a completely different account with fake sysvar data
        let (fake_sysvar_pubkey, fake_sysvar_account) =
            crate::test_utils::create_fake_instructions_sysvar_account(crate::ID);

        let instruction_data = crate::instruction::Initialize {
            access_manager: access_manager::ID,
        };

        let mut instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(router_state_pda, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(system_program::ID, false),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
            data: instruction_data.data(),
        };

        // Modify the instruction to reference the fake sysvar (simulating attacker control)
        instruction.accounts[3] = AccountMeta::new_readonly(fake_sysvar_pubkey, false);

        let accounts = vec![
            (
                router_state_pda,
                Account {
                    lamports: 0,
                    data: vec![],
                    owner: system_program::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            (
                payer,
                Account {
                    lamports: 10_000_000_000,
                    data: vec![],
                    owner: system_program::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            (
                system_program::ID,
                Account {
                    lamports: 0,
                    data: vec![],
                    owner: native_loader::ID,
                    executable: true,
                    rent_epoch: 0,
                },
            ),
            // Wormhole attack: provide a DIFFERENT account instead of the real sysvar
            (fake_sysvar_pubkey, fake_sysvar_account),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::test_utils::get_router_program_path());

        // Should be rejected by Anchor's address constraint check
        let checks = vec![Check::err(solana_sdk::program_error::ProgramError::Custom(
            anchor_lang::error::ErrorCode::ConstraintAddress as u32,
        ))];

        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }

    #[test]
    fn test_initialize_cpi_rejection() {
        let payer = Pubkey::new_unique();

        let (router_state_pda, _) = Pubkey::find_program_address(&[RouterState::SEED], &crate::ID);

        let instruction_data = crate::instruction::Initialize {
            access_manager: access_manager::ID,
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(router_state_pda, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(system_program::ID, false),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
            data: instruction_data.data(),
        };

        // Simulate CPI call from unauthorized program
        let malicious_program = Pubkey::new_unique();
        let (instruction, cpi_sysvar_account) = setup_cpi_call_test(instruction, malicious_program);

        let payer_lamports = 10_000_000_000;
        let accounts = vec![
            (
                router_state_pda,
                solana_sdk::account::Account {
                    lamports: 0,
                    data: vec![],
                    owner: system_program::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            (
                payer,
                solana_sdk::account::Account {
                    lamports: payer_lamports,
                    data: vec![],
                    owner: system_program::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            (
                system_program::ID,
                solana_sdk::account::Account {
                    lamports: 0,
                    data: vec![],
                    owner: native_loader::ID,
                    executable: true,
                    rent_epoch: 0,
                },
            ),
            cpi_sysvar_account,
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::test_utils::get_router_program_path());

        // When CPI is detected by reject_cpi, it returns RouterError::UnauthorizedSender (mapped from CpiValidationError::UnauthorizedCaller)
        let checks = vec![Check::err(ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + RouterError::UnauthorizedSender as u32,
        ))];
        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }
}
