use anchor_lang::prelude::*;
use solana_ibc_types::events::AccessManagerUpdated;

pub fn set_access_manager(
    ctx: Context<crate::SetAccessManager>,
    _chain_id: String,
    new_access_manager: Pubkey,
) -> Result<()> {
    let old_access_manager = ctx.accounts.client_state.access_manager;

    // Performs: CPI rejection + signer verification + role check
    access_manager::require_role(
        &ctx.accounts.access_manager,
        solana_ibc_types::roles::ADMIN_ROLE,
        &ctx.accounts.admin,
        &ctx.accounts.instructions_sysvar,
        &crate::ID,
    )?;

    ctx.accounts.client_state.access_manager = new_access_manager;

    emit!(AccessManagerUpdated {
        old_access_manager,
        new_access_manager,
    });

    msg!(
        "Access manager for client {} updated from {} to {}",
        ctx.accounts.client_state.chain_id,
        old_access_manager,
        new_access_manager
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::{fixtures::load_primary_fixtures, PROGRAM_BINARY_PATH};
    use crate::types::ClientState;
    use access_manager::AccessManagerError;
    use anchor_lang::InstructionData;
    use mollusk_svm::result::Check;
    use mollusk_svm::Mollusk;
    use solana_ibc_types::roles;
    use solana_sdk::account::Account as SolanaAccount;
    use solana_sdk::instruction::{AccountMeta, Instruction};
    use solana_sdk::pubkey::Pubkey;

    const ANCHOR_ERROR_OFFSET: u32 = 6000;

    fn create_signer_account() -> SolanaAccount {
        SolanaAccount {
            lamports: 1_000_000_000,
            data: vec![],
            owner: solana_sdk::system_program::ID,
            executable: false,
            rent_epoch: 0,
        }
    }

    fn create_client_state_account(chain_id: &str, access_manager: Pubkey) -> SolanaAccount {
        use anchor_lang::AccountSerialize;

        let (client_state, _, _) = load_primary_fixtures();
        let mut client_state = client_state;
        client_state.chain_id = chain_id.to_string();
        client_state.access_manager = access_manager;

        let mut data = vec![0u8; 8 + ClientState::INIT_SPACE];
        client_state.try_serialize(&mut &mut data[..]).unwrap();

        SolanaAccount {
            lamports: 10_000_000,
            data,
            owner: crate::ID,
            executable: false,
            rent_epoch: 0,
        }
    }

    fn create_access_manager_account(admin: Pubkey, role: u64) -> SolanaAccount {
        use access_manager::state::AccessManager;
        use anchor_lang::AccountSerialize;
        use solana_ibc_types::RoleData;

        let access_manager = AccessManager(solana_ibc_types::AccessManager {
            roles: vec![RoleData {
                role_id: role,
                members: vec![admin],
            }],
        });

        let mut data = vec![0u8; 8 + 10000]; // Enough space
        access_manager.try_serialize(&mut &mut data[..]).unwrap();

        SolanaAccount {
            lamports: 10_000_000,
            data,
            owner: access_manager::ID,
            executable: false,
            rent_epoch: 0,
        }
    }

    fn create_instructions_sysvar_account() -> (Pubkey, SolanaAccount) {
        use solana_sdk::sysvar::instructions::{
            construct_instructions_data, BorrowedAccountMeta, BorrowedInstruction,
        };

        let account_pubkey = Pubkey::new_unique();
        let account = BorrowedAccountMeta {
            pubkey: &account_pubkey,
            is_signer: false,
            is_writable: true,
        };
        let mock_instruction = BorrowedInstruction {
            program_id: &crate::ID,
            accounts: vec![account],
            data: &[],
        };

        let ixs_data = construct_instructions_data(&[mock_instruction]);

        (
            solana_sdk::sysvar::instructions::ID,
            SolanaAccount {
                lamports: 1_000_000,
                data: ixs_data,
                owner: solana_sdk::sysvar::ID,
                executable: false,
                rent_epoch: 0,
            },
        )
    }

    #[test]
    fn test_set_access_manager_success() {
        let chain_id = "test-chain";
        let admin = Pubkey::new_unique();
        let new_access_manager = Pubkey::new_unique();

        let (client_state_pda, _) =
            Pubkey::find_program_address(&[ClientState::SEED, chain_id.as_bytes()], &crate::ID);

        let (access_manager_pda, _) = Pubkey::find_program_address(
            &[access_manager::state::AccessManager::SEED],
            &access_manager::ID,
        );

        let instruction_data = crate::instruction::SetAccessManager {
            chain_id: chain_id.to_string(),
            new_access_manager,
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(client_state_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new_readonly(admin, true),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
            data: instruction_data.data(),
        };

        let client_state_account = create_client_state_account(chain_id, access_manager::ID);
        let access_manager_account = create_access_manager_account(admin, roles::ADMIN_ROLE);
        let (instructions_sysvar_pubkey, instructions_sysvar_account) =
            create_instructions_sysvar_account();

        let accounts = vec![
            (client_state_pda, client_state_account),
            (access_manager_pda, access_manager_account),
            (admin, create_signer_account()),
            (instructions_sysvar_pubkey, instructions_sysvar_account),
        ];

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::success()];
        let result = mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);

        let client_state_account = result
            .get_account(&client_state_pda)
            .expect("Client state account not found");
        let client_state: ClientState =
            ClientState::try_deserialize(&mut &client_state_account.data[..])
                .expect("Failed to deserialize client state");

        assert_eq!(client_state.access_manager, new_access_manager);
    }

    #[test]
    fn test_set_access_manager_not_admin() {
        let chain_id = "test-chain";
        let admin = Pubkey::new_unique();
        let non_admin = Pubkey::new_unique();
        let new_access_manager = Pubkey::new_unique();

        let (client_state_pda, _) =
            Pubkey::find_program_address(&[ClientState::SEED, chain_id.as_bytes()], &crate::ID);

        let (access_manager_pda, _) = Pubkey::find_program_address(
            &[access_manager::state::AccessManager::SEED],
            &access_manager::ID,
        );

        let instruction_data = crate::instruction::SetAccessManager {
            chain_id: chain_id.to_string(),
            new_access_manager,
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(client_state_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new_readonly(non_admin, true),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
            data: instruction_data.data(),
        };

        let client_state_account = create_client_state_account(chain_id, access_manager::ID);
        let access_manager_account = create_access_manager_account(admin, roles::ADMIN_ROLE);
        let (instructions_sysvar_pubkey, instructions_sysvar_account) =
            create_instructions_sysvar_account();

        let accounts = vec![
            (client_state_pda, client_state_account),
            (access_manager_pda, access_manager_account),
            (non_admin, create_signer_account()),
            (instructions_sysvar_pubkey, instructions_sysvar_account),
        ];

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(solana_sdk::program_error::ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + AccessManagerError::Unauthorized as u32,
        ))];

        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }
}
