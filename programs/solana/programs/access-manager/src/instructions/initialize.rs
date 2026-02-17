use crate::errors::AccessManagerError;
use crate::state::AccessManager;
use anchor_lang::prelude::*;
use solana_ibc_types::{reject_cpi, roles};

#[derive(Accounts)]
#[instruction(admin: Pubkey)]
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
    reject_cpi(&ctx.accounts.instructions_sysvar, &crate::ID).map_err(AccessManagerError::from)?;

    let access_manager = &mut ctx.accounts.access_manager;
    access_manager.roles = vec![];
    access_manager.whitelisted_programs = vec![];

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

#[cfg(test)]
mod integration_tests {
    use crate::test_utils::*;
    use anchor_lang::InstructionData;
    use solana_sdk::{
        instruction::{AccountMeta, Instruction},
        pubkey::Pubkey,
        signer::Signer,
    };

    fn setup_program_test_without_am() -> solana_program_test::ProgramTest {
        if std::env::var("SBF_OUT_DIR").is_err() {
            let deploy_dir = std::path::Path::new("../../target/deploy");
            std::env::set_var("SBF_OUT_DIR", deploy_dir);
        }

        let mut pt = solana_program_test::ProgramTest::new("access_manager", crate::ID, None);
        pt.add_program("test_cpi_proxy", TEST_CPI_PROXY_ID, None);
        pt.add_program("test_cpi_target", TEST_CPI_TARGET_ID, None);
        pt
    }

    fn build_initialize_ix(payer: Pubkey, admin: Pubkey) -> Instruction {
        let (access_manager_pda, _) =
            Pubkey::find_program_address(&[crate::state::AccessManager::SEED], &crate::ID);

        Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(access_manager_pda, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(solana_sdk::system_program::ID, false),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
            data: crate::instruction::Initialize { admin }.data(),
        }
    }

    #[tokio::test]
    async fn test_direct_call_succeeds() {
        let pt = setup_program_test_without_am();
        let (banks_client, payer, recent_blockhash) = pt.start().await;

        let admin = Pubkey::new_unique();
        let ix = build_initialize_ix(payer.pubkey(), admin);

        let tx = solana_sdk::transaction::Transaction::new_signed_with_payer(
            &[ix],
            Some(&payer.pubkey()),
            &[&payer],
            recent_blockhash,
        );
        let result = banks_client.process_transaction(tx).await;
        assert!(result.is_ok(), "Direct initialize should succeed");
    }

    #[tokio::test]
    async fn test_cpi_from_whitelisted_program_rejected() {
        let pt = setup_program_test_without_am();
        let (banks_client, payer, recent_blockhash) = pt.start().await;

        let admin = Pubkey::new_unique();
        let inner_ix = build_initialize_ix(payer.pubkey(), admin);
        let wrapped_ix = wrap_in_test_cpi_target_proxy(payer.pubkey(), &inner_ix);

        let tx = solana_sdk::transaction::Transaction::new_signed_with_payer(
            &[wrapped_ix],
            Some(&payer.pubkey()),
            &[&payer],
            recent_blockhash,
        );
        let err = banks_client.process_transaction(tx).await.unwrap_err();
        assert_eq!(
            extract_custom_error(&err),
            Some(ANCHOR_ERROR_OFFSET + crate::errors::AccessManagerError::CpiNotAllowed as u32),
        );
    }

    #[tokio::test]
    async fn test_cpi_from_unauthorized_program_rejected() {
        let pt = setup_program_test_without_am();
        let (banks_client, payer, recent_blockhash) = pt.start().await;

        let admin = Pubkey::new_unique();
        let inner_ix = build_initialize_ix(payer.pubkey(), admin);
        let wrapped_ix = wrap_in_test_cpi_proxy(payer.pubkey(), &inner_ix);

        let tx = solana_sdk::transaction::Transaction::new_signed_with_payer(
            &[wrapped_ix],
            Some(&payer.pubkey()),
            &[&payer],
            recent_blockhash,
        );
        let err = banks_client.process_transaction(tx).await.unwrap_err();
        assert_eq!(
            extract_custom_error(&err),
            Some(ANCHOR_ERROR_OFFSET + crate::errors::AccessManagerError::CpiNotAllowed as u32),
        );
    }
}
