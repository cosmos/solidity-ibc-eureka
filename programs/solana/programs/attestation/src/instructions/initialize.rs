use crate::error::ErrorCode;
use crate::types::{AccountVersion, AppState, ClientState};
use crate::ETH_ADDRESS_LEN;
use anchor_lang::prelude::*;

#[derive(Accounts)]
#[instruction(attestor_addresses: Vec<[u8; ETH_ADDRESS_LEN]>, min_required_sigs: u8, access_manager: Pubkey)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = payer,
        space = 8 + ClientState::INIT_SPACE,
        seeds = [ClientState::SEED],
        bump
    )]
    pub client_state: Account<'info, ClientState>,
    #[account(
        init,
        payer = payer,
        space = 8 + AppState::INIT_SPACE,
        seeds = [AppState::SEED],
        bump
    )]
    pub app_state: Account<'info, AppState>,
    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn initialize(
    ctx: Context<Initialize>,
    attestor_addresses: Vec<[u8; ETH_ADDRESS_LEN]>,
    min_required_sigs: u8,
    access_manager: Pubkey,
) -> Result<()> {
    require!(!attestor_addresses.is_empty(), ErrorCode::NoAttestors);
    require!(
        min_required_sigs > 0 && (min_required_sigs as usize) <= attestor_addresses.len(),
        ErrorCode::BadQuorum
    );

    let mut attestor_addresses = attestor_addresses;
    let original_len = attestor_addresses.len();
    attestor_addresses.sort_unstable();
    attestor_addresses.dedup();
    require!(
        attestor_addresses.len() == original_len,
        ErrorCode::DuplicateSigner
    );

    let client_state_account = &mut ctx.accounts.client_state;
    client_state_account.version = AccountVersion::V1;
    client_state_account.attestor_addresses = attestor_addresses;
    client_state_account.min_required_sigs = min_required_sigs;
    client_state_account.latest_height = 0;
    client_state_account.is_frozen = false;

    let app_state = &mut ctx.accounts.app_state;
    app_state.version = AccountVersion::V1;
    app_state.access_manager = access_manager;
    app_state._reserved = [0; 256];

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::accounts::{
        create_empty_account, create_payer_account, create_system_program_account,
    };
    use crate::test_helpers::PROGRAM_BINARY_PATH;
    use crate::types::{AppState, ClientState};
    use anchor_lang::solana_program::instruction::{AccountMeta, Instruction};
    use anchor_lang::InstructionData;
    use mollusk_svm::result::Check;
    use mollusk_svm::Mollusk;
    use solana_sdk::account::Account;
    use solana_sdk::pubkey::Pubkey;
    use solana_sdk::system_program;

    struct TestAccounts {
        payer: Pubkey,
        client_state_pda: Pubkey,
        app_state_pda: Pubkey,
        accounts: Vec<(Pubkey, Account)>,
    }

    fn setup_test_accounts() -> TestAccounts {
        let payer = Pubkey::new_unique();
        let client_state_pda = ClientState::pda();
        let app_state_pda = AppState::pda();

        let accounts = vec![
            (client_state_pda, create_empty_account()),
            (app_state_pda, create_empty_account()),
            (payer, create_payer_account()),
            (system_program::ID, create_system_program_account()),
        ];

        TestAccounts {
            payer,
            client_state_pda,
            app_state_pda,
            accounts,
        }
    }

    fn create_initialize_instruction(
        test_accounts: &TestAccounts,
        attestor_addresses: Vec<[u8; ETH_ADDRESS_LEN]>,
        min_required_sigs: u8,
    ) -> Instruction {
        let instruction_data = crate::instruction::Initialize {
            attestor_addresses,
            min_required_sigs,
            access_manager: access_manager::ID,
        };

        Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(test_accounts.client_state_pda, false),
                AccountMeta::new(test_accounts.app_state_pda, false),
                AccountMeta::new(test_accounts.payer, true),
                AccountMeta::new_readonly(system_program::ID, false),
            ],
            data: instruction_data.data(),
        }
    }

    fn expect_error(test_accounts: &TestAccounts, instruction: Instruction, error: ErrorCode) {
        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(anchor_lang::error::Error::from(error).into())];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    fn expect_success(test_accounts: &TestAccounts, instruction: Instruction) {
        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        mollusk.process_and_validate_instruction(
            &instruction,
            &test_accounts.accounts,
            &[Check::success()],
        );
    }

    #[test]
    fn test_initialize_happy_path() {
        let test_accounts = setup_test_accounts();
        let instruction =
            create_initialize_instruction(&test_accounts, vec![[1u8; 20], [2u8; 20], [3u8; 20]], 2);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![
            Check::success(),
            Check::account(&test_accounts.client_state_pda)
                .owner(&crate::ID)
                .build(),
            Check::account(&test_accounts.app_state_pda)
                .owner(&crate::ID)
                .build(),
        ];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[rstest::rstest]
    #[case::empty_attestors(vec![], 1, ErrorCode::NoAttestors)]
    #[case::zero_min_sigs(vec![[1u8; 20]], 0, ErrorCode::BadQuorum)]
    #[case::min_sigs_exceeds_attestors(vec![[1u8; 20]], 2, ErrorCode::BadQuorum)]
    #[case::duplicate_attestors(vec![[1u8; 20], [2u8; 20], [1u8; 20]], 2, ErrorCode::DuplicateSigner)]
    #[case::duplicate_attestors_adjacent(vec![[5u8; 20], [5u8; 20]], 1, ErrorCode::DuplicateSigner)]
    #[case::multiple_duplicate_groups(vec![[1u8; 20], [2u8; 20], [1u8; 20], [2u8; 20]], 2, ErrorCode::DuplicateSigner)]
    fn test_initialize_error(
        #[case] attestors: Vec<[u8; ETH_ADDRESS_LEN]>,
        #[case] min_sigs: u8,
        #[case] expected_error: ErrorCode,
    ) {
        let test_accounts = setup_test_accounts();
        let instruction = create_initialize_instruction(&test_accounts, attestors, min_sigs);
        expect_error(&test_accounts, instruction, expected_error);
    }

    #[rstest::rstest]
    #[case::single_attestor(vec![[0xAB; 20]], 1)]
    #[case::min_sigs_equals_count(vec![[1u8; 20], [2u8; 20], [3u8; 20]], 3)]
    #[case::max_attestors((0u8..20).map(|i| [i; 20]).collect::<Vec<_>>(), 10)]
    #[case::max_min_required_sigs((0u8..5).map(|i| [i; 20]).collect::<Vec<_>>(), 5)]
    fn test_initialize_success(
        #[case] attestors: Vec<[u8; ETH_ADDRESS_LEN]>,
        #[case] min_sigs: u8,
    ) {
        let test_accounts = setup_test_accounts();
        let instruction = create_initialize_instruction(&test_accounts, attestors, min_sigs);
        expect_success(&test_accounts, instruction);
    }
}
