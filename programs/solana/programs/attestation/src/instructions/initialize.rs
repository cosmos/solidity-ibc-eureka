use crate::error::ErrorCode;
use crate::state::ConsensusStateStore;
use crate::types::{AccountVersion, AppState, ClientState};
use crate::ETH_ADDRESS_LEN;
use anchor_lang::prelude::*;

#[derive(Accounts)]
#[instruction(latest_height: u64)]
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
        space = 8 + ConsensusStateStore::INIT_SPACE,
        seeds = [ConsensusStateStore::SEED, &latest_height.to_le_bytes()],
        bump
    )]
    pub consensus_state_store: Account<'info, ConsensusStateStore>,
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
    latest_height: u64,
    attestor_addresses: Vec<[u8; ETH_ADDRESS_LEN]>,
    min_required_sigs: u8,
    timestamp: u64,
    access_manager: Pubkey,
) -> Result<()> {
    require!(!attestor_addresses.is_empty(), ErrorCode::NoAttestors);
    require!(
        min_required_sigs > 0 && (min_required_sigs as usize) <= attestor_addresses.len(),
        ErrorCode::BadQuorum
    );
    require!(latest_height > 0, ErrorCode::InvalidHeight);
    require!(timestamp > 0, ErrorCode::InvalidTimestamp);

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
    client_state_account.latest_height = latest_height;
    client_state_account.is_frozen = false;

    let consensus_state_store = &mut ctx.accounts.consensus_state_store;
    consensus_state_store.height = latest_height;
    consensus_state_store.timestamp = timestamp;

    let app_state = &mut ctx.accounts.app_state;
    app_state.version = AccountVersion::V1;
    app_state.access_manager = access_manager;
    app_state._reserved = [0; 256];

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::ConsensusStateStore;
    use crate::test_helpers::accounts::{
        create_empty_account, create_payer_account, create_system_program_account,
    };
    use crate::test_helpers::fixtures::DEFAULT_TIMESTAMP;
    use crate::test_helpers::PROGRAM_BINARY_PATH;
    use crate::types::{AppState, ClientState};
    use anchor_lang::solana_program::instruction::{AccountMeta, Instruction};
    use anchor_lang::InstructionData;
    use mollusk_svm::result::Check;
    use mollusk_svm::Mollusk;
    use solana_sdk::account::Account;
    use solana_sdk::pubkey::Pubkey;
    use solana_sdk::system_program;

    const HEIGHT: u64 = 100;

    struct TestAccounts {
        payer: Pubkey,
        client_state_pda: Pubkey,
        consensus_state_store_pda: Pubkey,
        app_state_pda: Pubkey,
        accounts: Vec<(Pubkey, Account)>,
    }

    fn setup_test_accounts(latest_height: u64) -> TestAccounts {
        let payer = Pubkey::new_unique();
        let client_state_pda = ClientState::pda();
        let consensus_state_store_pda = ConsensusStateStore::pda(latest_height);
        let app_state_pda = AppState::pda();

        let accounts = vec![
            (client_state_pda, create_empty_account()),
            (consensus_state_store_pda, create_empty_account()),
            (app_state_pda, create_empty_account()),
            (payer, create_payer_account()),
            (system_program::ID, create_system_program_account()),
        ];

        TestAccounts {
            payer,
            client_state_pda,
            consensus_state_store_pda,
            app_state_pda,
            accounts,
        }
    }

    fn create_initialize_instruction(
        test_accounts: &TestAccounts,
        latest_height: u64,
        attestor_addresses: Vec<[u8; ETH_ADDRESS_LEN]>,
        min_required_sigs: u8,
        timestamp: u64,
    ) -> Instruction {
        let instruction_data = crate::instruction::Initialize {
            latest_height,
            attestor_addresses,
            min_required_sigs,
            timestamp,
            access_manager: access_manager::ID,
        };

        Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(test_accounts.client_state_pda, false),
                AccountMeta::new(test_accounts.consensus_state_store_pda, false),
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
        let test_accounts = setup_test_accounts(HEIGHT);
        let instruction = create_initialize_instruction(
            &test_accounts,
            HEIGHT,
            vec![[1u8; 20], [2u8; 20], [3u8; 20]],
            2,
            DEFAULT_TIMESTAMP,
        );

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![
            Check::success(),
            Check::account(&test_accounts.client_state_pda)
                .owner(&crate::ID)
                .build(),
            Check::account(&test_accounts.consensus_state_store_pda)
                .owner(&crate::ID)
                .build(),
            Check::account(&test_accounts.app_state_pda)
                .owner(&crate::ID)
                .build(),
        ];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[rstest::rstest]
    #[case::zero_height(0, vec![[1u8; 20]], 1, DEFAULT_TIMESTAMP, ErrorCode::InvalidHeight)]
    #[case::empty_attestors(HEIGHT, vec![], 1, DEFAULT_TIMESTAMP, ErrorCode::NoAttestors)]
    #[case::zero_min_sigs(HEIGHT, vec![[1u8; 20]], 0, DEFAULT_TIMESTAMP, ErrorCode::BadQuorum)]
    #[case::min_sigs_exceeds_attestors(HEIGHT, vec![[1u8; 20]], 2, DEFAULT_TIMESTAMP, ErrorCode::BadQuorum)]
    #[case::zero_timestamp(HEIGHT, vec![[1u8; 20]], 1, 0, ErrorCode::InvalidTimestamp)]
    #[case::duplicate_attestors(HEIGHT, vec![[1u8; 20], [2u8; 20], [1u8; 20]], 2, DEFAULT_TIMESTAMP, ErrorCode::DuplicateSigner)]
    #[case::duplicate_attestors_adjacent(HEIGHT, vec![[5u8; 20], [5u8; 20]], 1, DEFAULT_TIMESTAMP, ErrorCode::DuplicateSigner)]
    #[case::multiple_duplicate_groups(HEIGHT, vec![[1u8; 20], [2u8; 20], [1u8; 20], [2u8; 20]], 2, DEFAULT_TIMESTAMP, ErrorCode::DuplicateSigner)]
    fn test_initialize_error(
        #[case] height: u64,
        #[case] attestors: Vec<[u8; ETH_ADDRESS_LEN]>,
        #[case] min_sigs: u8,
        #[case] timestamp: u64,
        #[case] expected_error: ErrorCode,
    ) {
        let test_accounts = setup_test_accounts(height);
        let instruction =
            create_initialize_instruction(&test_accounts, height, attestors, min_sigs, timestamp);
        expect_error(&test_accounts, instruction, expected_error);
    }

    #[rstest::rstest]
    #[case::single_attestor(1, vec![[0xAB; 20]], 1, DEFAULT_TIMESTAMP)]
    #[case::min_sigs_equals_count(50, vec![[1u8; 20], [2u8; 20], [3u8; 20]], 3, DEFAULT_TIMESTAMP)]
    #[case::large_height(u64::MAX, vec![[1u8; 20]], 1, u64::MAX)]
    #[case::max_attestors(HEIGHT, (0u8..20).map(|i| [i; 20]).collect::<Vec<_>>(), 10, DEFAULT_TIMESTAMP)]
    #[case::max_min_required_sigs(HEIGHT, (0u8..5).map(|i| [i; 20]).collect::<Vec<_>>(), 5, DEFAULT_TIMESTAMP)]
    #[case::max_timestamp(HEIGHT, vec![[1u8; 20]], 1, u64::MAX)]
    fn test_initialize_success(
        #[case] height: u64,
        #[case] attestors: Vec<[u8; ETH_ADDRESS_LEN]>,
        #[case] min_sigs: u8,
        #[case] timestamp: u64,
    ) {
        let test_accounts = setup_test_accounts(height);
        let instruction =
            create_initialize_instruction(&test_accounts, height, attestors, min_sigs, timestamp);
        expect_success(&test_accounts, instruction);
    }
}
