use crate::state::AccessManager;
use crate::types::AccessManagerVersion;
use anchor_lang::prelude::*;

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
}

pub fn initialize(ctx: Context<Initialize>, admin: Pubkey) -> Result<()> {
    let access_manager = &mut ctx.accounts.access_manager;
    access_manager.version = AccessManagerVersion::V1;
    access_manager.admin = admin;
    access_manager.roles = vec![];
    access_manager._reserved = [0; 256];

    msg!("Global access control initialized with admin: {}", admin);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
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

        assert_eq!(deserialized_access_manager.admin, admin);
        assert_eq!(deserialized_access_manager.roles.len(), 0);
    }
}
