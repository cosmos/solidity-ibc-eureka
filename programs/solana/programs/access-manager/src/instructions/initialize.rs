use crate::errors::AccessManagerError;
use crate::state::AccessManager;
use anchor_lang::prelude::*;
use solana_ibc_types::{require_direct_call_or_whitelisted_caller, roles};

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = payer,
        space = 8 + AccessManager::INIT_SPACE,
        seeds = [AccessManager::SEED],
        bump
    )]
    pub access_manager: Account<'info, AccessManager>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,

    /// Instructions sysvar for CPI validation
    /// CHECK: Address constraint verifies this is the instructions sysvar
    #[account(address = anchor_lang::solana_program::sysvar::instructions::ID)]
    pub instructions_sysvar: AccountInfo<'info>,
}

pub fn initialize(ctx: Context<Initialize>, admin: Pubkey) -> Result<()> {
    // Validate caller
    require_direct_call_or_whitelisted_caller(
        &ctx.accounts.instructions_sysvar,
        crate::WHITELISTED_CPI_PROGRAMS,
        &crate::ID,
    )
    .map_err(AccessManagerError::from)?;

    let access_manager = &mut ctx.accounts.access_manager;
    access_manager.roles = vec![];

    // Grant ADMIN_ROLE to the initial admin
    access_manager.grant_role(roles::ADMIN_ROLE, admin)?;

    msg!("Global access control initialized with admin: {}", admin);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::*;
    use anchor_lang::InstructionData;
    use mollusk_svm::result::Check;
    use mollusk_svm::Mollusk;
    use solana_sdk::account::Account;
    use solana_sdk::instruction::{AccountMeta, Instruction};
    use solana_sdk::pubkey::Pubkey;
    use solana_sdk::{native_loader, system_program};

    #[test]
    fn test_initialize_happy_path() {
        let admin = Pubkey::new_unique();
        let payer = Pubkey::new_unique();

        let (access_manager_pda, _) =
            Pubkey::find_program_address(&[AccessManager::SEED], &crate::ID);

        let instruction_data = crate::instruction::Initialize { admin };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(access_manager_pda, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(system_program::ID, false),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
            data: instruction_data.data(),
        };

        let payer_lamports = 10_000_000_000;
        let accounts = vec![
            (
                access_manager_pda,
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
            (
                solana_sdk::sysvar::instructions::ID,
                crate::test_utils::create_instructions_sysvar_account(),
            ),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::get_access_manager_program_path());

        let checks = vec![
            Check::success(),
            Check::account(&access_manager_pda)
                .owner(&crate::ID)
                .build(),
        ];

        let result = mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);

        let access_manager_account = result
            .resulting_accounts
            .iter()
            .find(|(pubkey, _)| pubkey == &access_manager_pda)
            .map(|(_, account)| account)
            .expect("Access control account not found");

        let deserialized_access_manager: AccessManager =
            anchor_lang::AccountDeserialize::try_deserialize(&mut &access_manager_account.data[..])
                .expect("Failed to deserialize access control");

        // Verify admin has ADMIN_ROLE
        assert!(deserialized_access_manager.has_role(roles::ADMIN_ROLE, &admin));
        assert_eq!(deserialized_access_manager.roles.len(), 1);
    }

    #[test]
    fn test_initialize_fake_sysvar_wormhole_attack() {
        let admin = Pubkey::new_unique();
        let payer = Pubkey::new_unique();

        let (access_manager_pda, _) =
            Pubkey::find_program_address(&[AccessManager::SEED], &crate::ID);

        let instruction_data = crate::instruction::Initialize { admin };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(access_manager_pda, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(system_program::ID, false),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
            data: instruction_data.data(),
        };

        // Replace real sysvar with fake one (Wormhole-style attack)
        let (instruction, fake_sysvar_account) = setup_fake_sysvar_attack(instruction, crate::ID);

        let payer_lamports = 10_000_000_000;
        let accounts = vec![
            (
                access_manager_pda,
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
            fake_sysvar_account,
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::get_access_manager_program_path());

        mollusk.process_and_validate_instruction(
            &instruction,
            &accounts,
            &[expect_sysvar_attack_error()],
        );
    }

    #[test]
    fn test_initialize_cpi_rejection() {
        let payer = Pubkey::new_unique();
        let admin = Pubkey::new_unique();

        let (access_manager_pda, _) =
            Pubkey::find_program_address(&[AccessManager::SEED], &crate::ID);

        let instruction_data = crate::instruction::Initialize { admin };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(access_manager_pda, false),
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
                access_manager_pda,
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
            cpi_sysvar_account,
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::get_access_manager_program_path());

        mollusk.process_and_validate_instruction(
            &instruction,
            &accounts,
            &[expect_access_manager_cpi_rejection_error()],
        );
    }

    #[test]
    fn test_initialize_cannot_reinitialize() {
        let admin = Pubkey::new_unique();
        let payer = Pubkey::new_unique();

        // Use helper to create an already-initialized access manager
        let (access_manager_pda, access_manager_account) = create_initialized_access_manager(admin);

        let instruction_data = crate::instruction::Initialize { admin };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(access_manager_pda, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(system_program::ID, false),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
            data: instruction_data.data(),
        };

        let payer_lamports = 10_000_000_000;
        let accounts = vec![
            (access_manager_pda, access_manager_account),
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
            (
                solana_sdk::sysvar::instructions::ID,
                create_instructions_sysvar_account(),
            ),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::get_access_manager_program_path());

        // Anchor's `init` constraint fails when account already exists
        // Error code 0 means the account is already in use
        let checks = vec![Check::err(solana_sdk::program_error::ProgramError::Custom(
            0,
        ))];

        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }
}
