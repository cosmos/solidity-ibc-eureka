use crate::error::ErrorCode;
use crate::state::ConsensusStateStore;
use crate::types::{AccountVersion, AppState, ClientState, ConsensusState};
use anchor_lang::prelude::*;

#[derive(Accounts)]
#[instruction(client_id: String, latest_height: u64)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = payer,
        space = 8 + ClientState::INIT_SPACE,
        seeds = [ClientState::SEED, client_id.as_bytes()],
        bump
    )]
    pub client_state: Account<'info, ClientState>,
    #[account(
        init,
        payer = payer,
        space = 8 + ConsensusStateStore::INIT_SPACE,
        seeds = [ConsensusStateStore::SEED, client_state.key().as_ref(), &latest_height.to_le_bytes()],
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
    client_id: String,
    latest_height: u64,
    attestor_addresses: Vec<[u8; 20]>,
    min_required_sigs: u8,
    timestamp: u64,
    access_manager: Pubkey,
) -> Result<()> {
    require!(!client_id.is_empty(), ErrorCode::InvalidClientId);
    require!(!attestor_addresses.is_empty(), ErrorCode::NoAttestors);
    require!(
        min_required_sigs > 0 && (min_required_sigs as usize) <= attestor_addresses.len(),
        ErrorCode::BadQuorum
    );
    require!(latest_height > 0, ErrorCode::InvalidHeight);
    require!(timestamp > 0, ErrorCode::InvalidTimestamp);

    // Check for duplicate attestor addresses
    let has_duplicates = attestor_addresses.iter().enumerate().any(|(i, addr)| {
        attestor_addresses
            .iter()
            .skip(i.saturating_add(1))
            .any(|other| addr == other)
    });
    require!(!has_duplicates, ErrorCode::DuplicateSigner);

    let client_state_account = &mut ctx.accounts.client_state;
    client_state_account.version = AccountVersion::V1;
    client_state_account.client_id = client_id;
    client_state_account.attestor_addresses = attestor_addresses;
    client_state_account.min_required_sigs = min_required_sigs;
    client_state_account.latest_height = latest_height;
    client_state_account.is_frozen = false;

    let consensus_state_store = &mut ctx.accounts.consensus_state_store;
    consensus_state_store.height = latest_height;
    consensus_state_store.consensus_state = ConsensusState {
        height: latest_height,
        timestamp,
    };

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
    use crate::test_helpers::fixtures::{DEFAULT_CLIENT_ID, DEFAULT_TIMESTAMP};
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

    fn setup_test_accounts(client_id: &str, latest_height: u64) -> TestAccounts {
        let payer = Pubkey::new_unique();
        let client_state_pda = ClientState::pda(client_id);
        let consensus_state_store_pda = ConsensusStateStore::pda(&client_state_pda, latest_height);
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
        client_id: &str,
        latest_height: u64,
        attestor_addresses: Vec<[u8; 20]>,
        min_required_sigs: u8,
        timestamp: u64,
    ) -> Instruction {
        let instruction_data = crate::instruction::Initialize {
            client_id: client_id.to_string(),
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

    fn create_default_initialize_instruction(
        test_accounts: &TestAccounts,
        client_id: &str,
        latest_height: u64,
    ) -> Instruction {
        create_initialize_instruction(
            test_accounts,
            client_id,
            latest_height,
            vec![[1u8; 20]],
            1,
            DEFAULT_TIMESTAMP,
        )
    }

    #[test]
    fn test_initialize_happy_path() {
        let client_id = "attestation-client-0";
        let test_accounts = setup_test_accounts(client_id, HEIGHT);
        let instruction = create_initialize_instruction(
            &test_accounts,
            client_id,
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

    #[test]
    fn test_initialize_empty_client_id() {
        let client_id = "";
        let test_accounts = setup_test_accounts(client_id, HEIGHT);
        let instruction = create_default_initialize_instruction(&test_accounts, client_id, HEIGHT);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(
            anchor_lang::error::Error::from(ErrorCode::InvalidClientId).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_initialize_empty_attestors() {
        let test_accounts = setup_test_accounts(DEFAULT_CLIENT_ID, HEIGHT);
        let instruction = create_initialize_instruction(
            &test_accounts,
            DEFAULT_CLIENT_ID,
            HEIGHT,
            vec![],
            1,
            DEFAULT_TIMESTAMP,
        );

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(
            anchor_lang::error::Error::from(ErrorCode::NoAttestors).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_initialize_zero_min_sigs() {
        let test_accounts = setup_test_accounts(DEFAULT_CLIENT_ID, HEIGHT);
        let instruction = create_initialize_instruction(
            &test_accounts,
            DEFAULT_CLIENT_ID,
            HEIGHT,
            vec![[1u8; 20]],
            0,
            DEFAULT_TIMESTAMP,
        );

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(
            anchor_lang::error::Error::from(ErrorCode::BadQuorum).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_initialize_min_sigs_exceeds_attestors() {
        let test_accounts = setup_test_accounts(DEFAULT_CLIENT_ID, HEIGHT);
        let instruction = create_initialize_instruction(
            &test_accounts,
            DEFAULT_CLIENT_ID,
            HEIGHT,
            vec![[1u8; 20]],
            2,
            DEFAULT_TIMESTAMP,
        );

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(
            anchor_lang::error::Error::from(ErrorCode::BadQuorum).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_initialize_zero_height() {
        let zero_height = 0u64;
        let test_accounts = setup_test_accounts(DEFAULT_CLIENT_ID, zero_height);
        let instruction =
            create_default_initialize_instruction(&test_accounts, DEFAULT_CLIENT_ID, zero_height);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(
            anchor_lang::error::Error::from(ErrorCode::InvalidHeight).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_initialize_single_attestor() {
        let client_id = "single-attestor-client";
        let test_accounts = setup_test_accounts(client_id, 1);
        let instruction = create_initialize_instruction(
            &test_accounts,
            client_id,
            1,
            vec![[0xAB; 20]],
            1,
            DEFAULT_TIMESTAMP,
        );

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::success()];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_initialize_min_sigs_equals_attestor_count() {
        let client_id = "exact-sigs-client";
        let test_accounts = setup_test_accounts(client_id, 50);
        let instruction = create_initialize_instruction(
            &test_accounts,
            client_id,
            50,
            vec![[1u8; 20], [2u8; 20], [3u8; 20]],
            3,
            DEFAULT_TIMESTAMP,
        );

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::success()];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_initialize_large_height() {
        let client_id = "large-height-client";
        let max_height = u64::MAX;
        let test_accounts = setup_test_accounts(client_id, max_height);
        let instruction = create_initialize_instruction(
            &test_accounts,
            client_id,
            max_height,
            vec![[1u8; 20]],
            1,
            u64::MAX,
        );

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::success()];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_initialize_zero_timestamp() {
        let test_accounts = setup_test_accounts(DEFAULT_CLIENT_ID, HEIGHT);
        let instruction = create_initialize_instruction(
            &test_accounts,
            DEFAULT_CLIENT_ID,
            HEIGHT,
            vec![[1u8; 20]],
            1,
            0, // zero timestamp
        );

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(
            anchor_lang::error::Error::from(ErrorCode::InvalidTimestamp).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_initialize_duplicate_attestors() {
        let test_accounts = setup_test_accounts(DEFAULT_CLIENT_ID, HEIGHT);
        let instruction = create_initialize_instruction(
            &test_accounts,
            DEFAULT_CLIENT_ID,
            HEIGHT,
            vec![[1u8; 20], [2u8; 20], [1u8; 20]],
            2,
            DEFAULT_TIMESTAMP,
        );

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(
            anchor_lang::error::Error::from(ErrorCode::DuplicateSigner).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_initialize_duplicate_attestors_adjacent() {
        let test_accounts = setup_test_accounts(DEFAULT_CLIENT_ID, HEIGHT);
        let instruction = create_initialize_instruction(
            &test_accounts,
            DEFAULT_CLIENT_ID,
            HEIGHT,
            vec![[5u8; 20], [5u8; 20]],
            1,
            DEFAULT_TIMESTAMP,
        );

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(
            anchor_lang::error::Error::from(ErrorCode::DuplicateSigner).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_initialize_max_attestors() {
        let client_id = "max-attestors-client";
        let test_accounts = setup_test_accounts(client_id, HEIGHT);

        // Create 20 unique attestor addresses (max_len from ClientState)
        let attestors: Vec<[u8; 20]> = (0u8..20).map(|i| [i; 20]).collect();

        let instruction = create_initialize_instruction(
            &test_accounts,
            client_id,
            HEIGHT,
            attestors,
            10, // Require half for quorum
            DEFAULT_TIMESTAMP,
        );

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::success()];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_initialize_max_min_required_sigs() {
        let client_id = "max-sigs-client";
        let test_accounts = setup_test_accounts(client_id, HEIGHT);

        // 5 attestors, require all 5 signatures
        let attestors: Vec<[u8; 20]> = (0u8..5).map(|i| [i; 20]).collect();

        let instruction = create_initialize_instruction(
            &test_accounts,
            client_id,
            HEIGHT,
            attestors,
            5,
            DEFAULT_TIMESTAMP,
        );

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::success()];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_initialize_long_client_id() {
        // PDA seeds have length limits, so test a reasonably long client_id
        let client_id = "a]".repeat(16); // 32 chars
        let test_accounts = setup_test_accounts(&client_id, HEIGHT);

        let instruction = create_initialize_instruction(
            &test_accounts,
            &client_id,
            HEIGHT,
            vec![[1u8; 20]],
            1,
            DEFAULT_TIMESTAMP,
        );

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::success()];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_initialize_max_height() {
        let client_id = "max-height-client";
        let max_height = u64::MAX;
        let test_accounts = setup_test_accounts(client_id, max_height);

        let instruction = create_initialize_instruction(
            &test_accounts,
            client_id,
            max_height,
            vec![[1u8; 20]],
            1,
            DEFAULT_TIMESTAMP,
        );

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::success()];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_initialize_max_timestamp() {
        let client_id = "max-timestamp-client";
        let test_accounts = setup_test_accounts(client_id, HEIGHT);

        let instruction = create_initialize_instruction(
            &test_accounts,
            client_id,
            HEIGHT,
            vec![[1u8; 20]],
            1,
            u64::MAX,
        );

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::success()];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }
}
