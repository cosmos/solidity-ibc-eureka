use crate::errors::RouterError;
use crate::events::{ClientAddedEvent, ClientUpdatedEvent};
use crate::state::{AccountVersion, Client, ClientSequence, CounterpartyInfo, RouterState};
use anchor_lang::prelude::*;

#[derive(Accounts)]
#[instruction(client_id: String)]
pub struct AddClient<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

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
        payer = authority,
        space = 8 + Client::INIT_SPACE,
        seeds = [Client::SEED, client_id.as_bytes()],
        bump,
    )]
    pub client: Account<'info, Client>,

    #[account(
        init,
        payer = authority,
        space = 8 + ClientSequence::INIT_SPACE,
        seeds = [ClientSequence::SEED, client_id.as_bytes()],
        bump,
    )]
    pub client_sequence: Account<'info, ClientSequence>,

    pub relayer: Signer<'info>,

    /// CHECK: Light client program ID validation happens in instruction
    pub light_client_program: AccountInfo<'info>,

    pub system_program: Program<'info, System>,

    /// Instructions sysvar for CPI validation
    /// CHECK: Address constraint verifies this is the instructions sysvar
    #[account(address = anchor_lang::solana_program::sysvar::instructions::ID)]
    pub instructions_sysvar: AccountInfo<'info>,
}

#[derive(Accounts)]
#[instruction(client_id: String)]
pub struct MigrateClient<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

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
        seeds = [Client::SEED, client_id.as_bytes()],
        bump
    )]
    pub client: Account<'info, Client>,

    pub relayer: Signer<'info>,

    /// Instructions sysvar for CPI validation
    /// CHECK: Address constraint verifies this is the instructions sysvar
    #[account(address = anchor_lang::solana_program::sysvar::instructions::ID)]
    pub instructions_sysvar: AccountInfo<'info>,
}

/// Parameters for migrating a client
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct MigrateClientParams {
    /// New light client program ID (None = keep current)
    pub client_program_id: Option<Pubkey>,
    /// New counterparty info (None = keep current)
    pub counterparty_info: Option<CounterpartyInfo>,
    /// New active status (None = keep current)
    pub active: Option<bool>,
}

pub fn add_client(
    ctx: Context<AddClient>,
    client_id: String,
    counterparty_info: CounterpartyInfo,
) -> Result<()> {
    access_manager::require_role(
        &ctx.accounts.access_manager,
        solana_ibc_types::roles::ID_CUSTOMIZER_ROLE,
        &ctx.accounts.relayer,
        &ctx.accounts.instructions_sysvar,
        &crate::ID,
    )?;

    let client = &mut ctx.accounts.client;
    let light_client_program = &ctx.accounts.light_client_program;

    require!(
        validate_custom_ibc_identifier(&client_id),
        RouterError::InvalidClientId
    );

    // The client account creation with init constraint ensures the client doesn't already exist
    // If it exists, the init will fail with "account already in use" error

    require!(
        !counterparty_info.merkle_prefix.is_empty(),
        RouterError::InvalidCounterpartyInfo
    );

    client.version = AccountVersion::V1;
    client.client_id = client_id;
    client.client_program_id = light_client_program.key();
    client.counterparty_info = counterparty_info;
    client.active = true;
    client._reserved = [0u8; 256];

    // Initialize client sequence to start from 1 (IBC sequences start from 1, not 0)
    let client_sequence = &mut ctx.accounts.client_sequence;
    client_sequence.next_sequence_send = 1;

    emit!(ClientAddedEvent {
        client: client.to_client_account(),
    });

    Ok(())
}

pub fn migrate_client(
    ctx: Context<MigrateClient>,
    _client_id: String,
    params: MigrateClientParams,
) -> Result<()> {
    let client = &mut ctx.accounts.client;

    access_manager::require_admin(
        &ctx.accounts.access_manager,
        &ctx.accounts.relayer,
        &ctx.accounts.instructions_sysvar,
        &crate::ID,
    )?;

    require!(
        params.client_program_id.is_some()
            || params.counterparty_info.is_some()
            || params.active.is_some(),
        RouterError::InvalidMigrationParams
    );

    if let Some(new_program_id) = params.client_program_id {
        client.client_program_id = new_program_id;
    }

    if let Some(new_counterparty_info) = params.counterparty_info.clone() {
        require!(
            !new_counterparty_info.merkle_prefix.is_empty(),
            RouterError::InvalidCounterpartyInfo
        );
        client.counterparty_info = new_counterparty_info;
    }

    if let Some(new_active) = params.active {
        client.active = new_active;
    }

    emit!(ClientUpdatedEvent {
        client: client.to_client_account(),
    });

    Ok(())
}

const CLIENT_ID_PREFIX: &str = "client-";
const CHANNEL_ID_PREFIX: &str = "channel-";

// TODO: move to another crate
/// Validates a custom IBC identifier
/// - Length must be between 4 and 128 characters
/// - Must NOT start with "client-" or "channel-" (reserved prefixes)
/// - Can only contain:
///   - Alphanumeric characters (a-z, A-Z, 0-9)
///   - Special characters: ., _, +, -, #, [, ], <, >
pub fn validate_custom_ibc_identifier(custom_id: &str) -> bool {
    if custom_id.trim().is_empty() {
        return false;
    }

    let bytes = custom_id.as_bytes();

    if bytes.len() < 4 || bytes.len() > 128 {
        return false;
    }

    if custom_id.starts_with(CLIENT_ID_PREFIX) || custom_id.starts_with(CHANNEL_ID_PREFIX) {
        return false;
    }

    for &c in bytes {
        let valid = matches!(c,
            b'a'..=b'z' |    // a-z
            b'0'..=b'9' |    // 0-9
            b'A'..=b'Z' |    // A-Z
            b'.' | b'_' | b'+' | b'-' |    // ., _, +, -
            b'#' | b'[' | b']' | b'<' | b'>'    // #, [, ], <, >
        );

        if !valid {
            return false;
        }
    }

    true
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

    /// Helper struct for test configuration
    struct AddClientTestConfig<'a> {
        client_id: &'a str,
        counterparty_info: Option<CounterpartyInfo>,
        expected_error: Option<RouterError>,
    }

    impl<'a> AddClientTestConfig<'a> {
        fn expecting_error(client_id: &'a str, error: RouterError) -> Self {
            Self {
                client_id,
                counterparty_info: Some(Self::valid_counterparty_info()),
                expected_error: Some(error),
            }
        }

        fn with_counterparty_info(
            client_id: &'a str,
            info: CounterpartyInfo,
            error: RouterError,
        ) -> Self {
            Self {
                client_id,
                counterparty_info: Some(info),
                expected_error: Some(error),
            }
        }

        fn valid_counterparty_info() -> CounterpartyInfo {
            CounterpartyInfo {
                client_id: "counterparty-client".to_string(),
                merkle_prefix: vec![vec![0x01, 0x02, 0x03]],
            }
        }
    }

    fn test_add_client(config: AddClientTestConfig) -> mollusk_svm::result::InstructionResult {
        let authority = Pubkey::new_unique();
        let relayer = authority;
        let light_client_program = Pubkey::new_unique();

        let (router_state_pda, router_state_data) = setup_router_state();
        let (access_manager_pda, access_manager_data) =
            setup_access_manager_with_roles(&[(roles::ID_CUSTOMIZER_ROLE, &[authority])]);
        let (client_pda, _) =
            Pubkey::find_program_address(&[Client::SEED, config.client_id.as_bytes()], &crate::ID);
        let (client_sequence_pda, _) = Pubkey::find_program_address(
            &[ClientSequence::SEED, config.client_id.as_bytes()],
            &crate::ID,
        );

        let instruction_data = crate::instruction::AddClient {
            client_id: config.client_id.to_string(),
            counterparty_info: config
                .counterparty_info
                .unwrap_or_else(AddClientTestConfig::valid_counterparty_info),
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(authority, true),
                AccountMeta::new_readonly(router_state_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new(client_pda, false),
                AccountMeta::new(client_sequence_pda, false),
                AccountMeta::new_readonly(relayer, true),
                AccountMeta::new_readonly(light_client_program, false),
                AccountMeta::new_readonly(system_program::ID, false),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_system_account(authority),
            create_account(router_state_pda, router_state_data, crate::ID),
            create_account(access_manager_pda, access_manager_data, access_manager::ID),
            create_uninitialized_account(client_pda, 0),
            create_uninitialized_account(client_sequence_pda, 0),
            create_system_account(relayer),
            create_program_account(light_client_program),
            create_program_account(system_program::ID),
            create_instructions_sysvar_account_with_caller(crate::ID),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::test_utils::get_router_program_path());

        let checks = config.expected_error.map_or_else(
            || {
                vec![
                    Check::success(),
                    Check::account(&client_pda).owner(&crate::ID).build(),
                    Check::account(&client_sequence_pda)
                        .owner(&crate::ID)
                        .build(),
                ]
            },
            |error| {
                vec![Check::err(ProgramError::Custom(
                    ANCHOR_ERROR_OFFSET + error as u32,
                ))]
            },
        );

        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks)
    }

    #[test]
    fn test_add_client_happy_path() {
        let client_id = "test-client-01";
        let counterparty_info = CounterpartyInfo {
            client_id: "counterparty-client".to_string(),
            merkle_prefix: vec![vec![0x01, 0x02, 0x03]],
        };

        let result = test_add_client(AddClientTestConfig {
            client_id,
            counterparty_info: Some(counterparty_info.clone()),
            expected_error: None,
        });

        // Get the accounts from the result to verify everything worked
        let authority = result
            .resulting_accounts
            .iter()
            .find(|(_, account)| {
                account.owner == system_program::ID && account.lamports > 1_000_000_000
            })
            .map(|(pubkey, _)| *pubkey)
            .expect("Authority account not found");

        let (client_pda, _) =
            Pubkey::find_program_address(&[Client::SEED, client_id.as_bytes()], &crate::ID);
        let (client_sequence_pda, _) =
            Pubkey::find_program_address(&[ClientSequence::SEED, client_id.as_bytes()], &crate::ID);

        // Verify authority paid for account creation
        let authority_account = result
            .resulting_accounts
            .iter()
            .find(|(pubkey, _)| pubkey == &authority)
            .map(|(_, account)| account)
            .expect("Authority account should exist");

        // Authority should have less lamports after paying for account creation
        assert!(
            authority_account.lamports < 10_000_000_000,
            "Authority should have paid for account creation"
        );

        // Verify Client account was created correctly
        let client_account = result
            .resulting_accounts
            .iter()
            .find(|(pubkey, _)| pubkey == &client_pda)
            .map(|(_, account)| account)
            .expect("Client account not found");

        assert_eq!(
            client_account.owner,
            crate::ID,
            "Client account should be owned by program"
        );
        assert!(
            client_account.lamports > 0,
            "Client account should be rent-exempt"
        );

        let deserialized_client: Client = Client::try_deserialize(&mut &client_account.data[..])
            .expect("Failed to deserialize client");

        assert_eq!(deserialized_client.client_id, client_id);
        assert!(deserialized_client.active);
        assert_eq!(
            deserialized_client.counterparty_info.client_id,
            counterparty_info.client_id
        );
        assert_eq!(
            deserialized_client.counterparty_info.merkle_prefix,
            counterparty_info.merkle_prefix
        );
        // Just verify that a light client program was set
        assert_ne!(deserialized_client.client_program_id, Pubkey::default());

        // Verify ClientSequence account was created correctly
        let client_sequence_account = result
            .resulting_accounts
            .iter()
            .find(|(pubkey, _)| pubkey == &client_sequence_pda)
            .map(|(_, account)| account)
            .expect("ClientSequence account not found");

        assert_eq!(
            client_sequence_account.owner,
            crate::ID,
            "ClientSequence account should be owned by program"
        );
        assert!(
            client_sequence_account.lamports > 0,
            "ClientSequence account should be rent-exempt"
        );

        let deserialized_sequence: ClientSequence =
            ClientSequence::try_deserialize(&mut &client_sequence_account.data[..])
                .expect("Failed to deserialize client sequence");

        assert_eq!(
            deserialized_sequence.next_sequence_send, 1,
            "Sequence should be initialized to 1"
        );
    }

    #[test]
    fn test_add_client_invalid_client_id_too_short() {
        test_add_client(AddClientTestConfig::expecting_error(
            "abc", // Too short, min is 4 chars
            RouterError::InvalidClientId,
        ));
    }

    #[test]
    fn test_add_client_invalid_client_id_reserved_prefix() {
        test_add_client(AddClientTestConfig::expecting_error(
            "client-123", // Invalid: uses reserved prefix
            RouterError::InvalidClientId,
        ));
    }

    #[test]
    fn test_add_client_invalid_client_id_invalid_chars() {
        test_add_client(AddClientTestConfig::expecting_error(
            "test@client", // Invalid character @
            RouterError::InvalidClientId,
        ));
    }

    #[test]
    fn test_add_client_invalid_counterparty_info_empty_merkle_prefix() {
        test_add_client(AddClientTestConfig::with_counterparty_info(
            "test-client-04",
            CounterpartyInfo {
                client_id: "counterparty-client".to_string(),
                merkle_prefix: vec![], // Invalid: empty
            },
            RouterError::InvalidCounterpartyInfo,
        ));
    }

    #[test]
    fn test_migrate_client_active_status() {
        let authority = Pubkey::new_unique();
        let relayer = authority;
        let light_client_program = Pubkey::new_unique();
        let client_id = "test-client-02";

        let (router_state_pda, router_state_data) = setup_router_state();
        let (access_manager_pda, access_manager_data) =
            setup_access_manager_with_roles(&[(roles::ADMIN_ROLE, &[authority])]);
        let (client_pda, client_data) =
            setup_client(client_id, light_client_program, "counterparty-client", true);

        let instruction_data = crate::instruction::MigrateClient {
            client_id: client_id.to_string(),
            params: MigrateClientParams {
                client_program_id: None,
                counterparty_info: None,
                active: Some(false), // Deactivate the client
            },
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(authority, true),
                AccountMeta::new_readonly(router_state_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new(client_pda, false),
                AccountMeta::new_readonly(relayer, true),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_system_account(authority),
            create_account(router_state_pda, router_state_data, crate::ID),
            create_account(access_manager_pda, access_manager_data, access_manager::ID),
            create_account(client_pda, client_data, crate::ID),
            create_system_account(relayer),
            create_instructions_sysvar_account_with_caller(crate::ID),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::test_utils::get_router_program_path());

        let result =
            mollusk.process_and_validate_instruction(&instruction, &accounts, &[Check::success()]);

        // Verify client was updated
        let client_account = result
            .resulting_accounts
            .iter()
            .find(|(pubkey, _)| pubkey == &client_pda)
            .map(|(_, account)| account)
            .expect("Client account not found");

        let deserialized_client: Client = Client::try_deserialize(&mut &client_account.data[..])
            .expect("Failed to deserialize client");

        assert!(!deserialized_client.active, "Client should be deactivated");
        assert_eq!(deserialized_client.client_id, client_id);
        assert_eq!(deserialized_client.client_program_id, light_client_program);
    }

    #[test]
    fn test_migrate_client_update_program_id() {
        let authority = Pubkey::new_unique();
        let relayer = authority;
        let old_light_client_program = Pubkey::new_unique();
        let new_light_client_program = Pubkey::new_unique();
        let client_id = "test-client-03";

        let (router_state_pda, router_state_data) = setup_router_state();
        let (access_manager_pda, access_manager_data) =
            setup_access_manager_with_roles(&[(roles::ADMIN_ROLE, &[authority])]);
        let (client_pda, client_data) = setup_client(
            client_id,
            old_light_client_program,
            "counterparty-client",
            true,
        );

        let instruction_data = crate::instruction::MigrateClient {
            client_id: client_id.to_string(),
            params: MigrateClientParams {
                client_program_id: Some(new_light_client_program),
                counterparty_info: None,
                active: None,
            },
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(authority, true),
                AccountMeta::new_readonly(router_state_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new(client_pda, false),
                AccountMeta::new_readonly(relayer, true),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_system_account(authority),
            create_account(router_state_pda, router_state_data, crate::ID),
            create_account(access_manager_pda, access_manager_data, access_manager::ID),
            create_account(client_pda, client_data, crate::ID),
            create_system_account(relayer),
            create_instructions_sysvar_account_with_caller(crate::ID),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::test_utils::get_router_program_path());

        let result =
            mollusk.process_and_validate_instruction(&instruction, &accounts, &[Check::success()]);

        let client_account = result
            .resulting_accounts
            .iter()
            .find(|(pubkey, _)| pubkey == &client_pda)
            .map(|(_, account)| account)
            .expect("Client account not found");

        let deserialized_client: Client = Client::try_deserialize(&mut &client_account.data[..])
            .expect("Failed to deserialize client");

        assert_eq!(
            deserialized_client.client_program_id, new_light_client_program,
            "Client program ID should be updated"
        );
        assert_eq!(deserialized_client.client_id, client_id);
        assert!(deserialized_client.active);
    }

    #[test]
    fn test_migrate_client_update_counterparty_info() {
        let authority = Pubkey::new_unique();
        let relayer = authority;
        let light_client_program = Pubkey::new_unique();
        let client_id = "test-client-04";

        let new_counterparty_info = CounterpartyInfo {
            client_id: "new-counterparty".to_string(),
            merkle_prefix: vec![vec![0x02, 0x03]],
        };

        let (router_state_pda, router_state_data) = setup_router_state();
        let (access_manager_pda, access_manager_data) =
            setup_access_manager_with_roles(&[(roles::ADMIN_ROLE, &[authority])]);
        let (client_pda, client_data) =
            setup_client(client_id, light_client_program, "old-counterparty", true);

        let instruction_data = crate::instruction::MigrateClient {
            client_id: client_id.to_string(),
            params: MigrateClientParams {
                client_program_id: None,
                counterparty_info: Some(new_counterparty_info.clone()),
                active: None,
            },
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(authority, true),
                AccountMeta::new_readonly(router_state_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new(client_pda, false),
                AccountMeta::new_readonly(relayer, true),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_system_account(authority),
            create_account(router_state_pda, router_state_data, crate::ID),
            create_account(access_manager_pda, access_manager_data, access_manager::ID),
            create_account(client_pda, client_data, crate::ID),
            create_system_account(relayer),
            create_instructions_sysvar_account_with_caller(crate::ID),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::test_utils::get_router_program_path());

        let result =
            mollusk.process_and_validate_instruction(&instruction, &accounts, &[Check::success()]);

        let client_account = result
            .resulting_accounts
            .iter()
            .find(|(pubkey, _)| pubkey == &client_pda)
            .map(|(_, account)| account)
            .expect("Client account not found");

        let deserialized_client: Client = Client::try_deserialize(&mut &client_account.data[..])
            .expect("Failed to deserialize client");

        assert_eq!(
            deserialized_client.counterparty_info.client_id, new_counterparty_info.client_id,
            "Counterparty client ID should be updated"
        );
        assert_eq!(
            deserialized_client.counterparty_info.merkle_prefix,
            new_counterparty_info.merkle_prefix,
            "Merkle prefix should be updated"
        );
    }

    #[test]
    fn test_migrate_client_update_all_fields() {
        let authority = Pubkey::new_unique();
        let relayer = authority;
        let old_light_client_program = Pubkey::new_unique();
        let new_light_client_program = Pubkey::new_unique();
        let client_id = "test-client-06";

        let new_counterparty_info = CounterpartyInfo {
            client_id: "new-counterparty".to_string(),
            merkle_prefix: vec![vec![0x04, 0x05, 0x06]],
        };

        let (router_state_pda, router_state_data) = setup_router_state();
        let (access_manager_pda, access_manager_data) =
            setup_access_manager_with_roles(&[(roles::ADMIN_ROLE, &[authority])]);
        let (client_pda, client_data) = setup_client(
            client_id,
            old_light_client_program,
            "old-counterparty",
            true,
        );

        let instruction_data = crate::instruction::MigrateClient {
            client_id: client_id.to_string(),
            params: MigrateClientParams {
                client_program_id: Some(new_light_client_program),
                counterparty_info: Some(new_counterparty_info.clone()),
                active: Some(false),
            },
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(authority, true),
                AccountMeta::new_readonly(router_state_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new(client_pda, false),
                AccountMeta::new_readonly(relayer, true),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_system_account(authority),
            create_account(router_state_pda, router_state_data, crate::ID),
            create_account(access_manager_pda, access_manager_data, access_manager::ID),
            create_account(client_pda, client_data, crate::ID),
            create_system_account(relayer),
            create_instructions_sysvar_account_with_caller(crate::ID),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::test_utils::get_router_program_path());

        let result =
            mollusk.process_and_validate_instruction(&instruction, &accounts, &[Check::success()]);

        let client_account = result
            .resulting_accounts
            .iter()
            .find(|(pubkey, _)| pubkey == &client_pda)
            .map(|(_, account)| account)
            .expect("Client account not found");

        let deserialized_client: Client = Client::try_deserialize(&mut &client_account.data[..])
            .expect("Failed to deserialize client");

        assert_eq!(
            deserialized_client.client_program_id,
            new_light_client_program
        );
        assert_eq!(
            deserialized_client.counterparty_info.client_id,
            new_counterparty_info.client_id
        );
        assert_eq!(
            deserialized_client.counterparty_info.merkle_prefix,
            new_counterparty_info.merkle_prefix
        );
        assert!(!deserialized_client.active);
    }

    #[test]
    fn test_migrate_client_no_params_fails() {
        let authority = Pubkey::new_unique();
        let relayer = authority;
        let light_client_program = Pubkey::new_unique();
        let client_id = "test-client-07";

        let (router_state_pda, router_state_data) = setup_router_state();
        let (access_manager_pda, access_manager_data) =
            setup_access_manager_with_roles(&[(roles::ADMIN_ROLE, &[authority])]);
        let (client_pda, client_data) =
            setup_client(client_id, light_client_program, "counterparty-client", true);

        let instruction_data = crate::instruction::MigrateClient {
            client_id: client_id.to_string(),
            params: MigrateClientParams {
                client_program_id: None,
                counterparty_info: None,
                active: None,
            },
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(authority, true),
                AccountMeta::new_readonly(router_state_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new(client_pda, false),
                AccountMeta::new_readonly(relayer, true),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_system_account(authority),
            create_account(router_state_pda, router_state_data, crate::ID),
            create_account(access_manager_pda, access_manager_data, access_manager::ID),
            create_account(client_pda, client_data, crate::ID),
            create_system_account(relayer),
            create_instructions_sysvar_account_with_caller(crate::ID),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::test_utils::get_router_program_path());

        let checks = vec![Check::err(ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + RouterError::InvalidMigrationParams as u32,
        ))];

        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }

    #[test]
    fn test_migrate_client_invalid_counterparty_info() {
        let authority = Pubkey::new_unique();
        let relayer = authority;
        let light_client_program = Pubkey::new_unique();
        let client_id = "test-client-08";

        let (router_state_pda, router_state_data) = setup_router_state();
        let (access_manager_pda, access_manager_data) =
            setup_access_manager_with_roles(&[(roles::ADMIN_ROLE, &[authority])]);
        let (client_pda, client_data) =
            setup_client(client_id, light_client_program, "counterparty-client", true);

        let instruction_data = crate::instruction::MigrateClient {
            client_id: client_id.to_string(),
            params: MigrateClientParams {
                client_program_id: None,
                counterparty_info: Some(CounterpartyInfo {
                    client_id: "new-counterparty".to_string(),
                    merkle_prefix: vec![], // Invalid: empty
                }),
                active: None,
            },
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(authority, true),
                AccountMeta::new_readonly(router_state_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new(client_pda, false),
                AccountMeta::new_readonly(relayer, true),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_system_account(authority),
            create_account(router_state_pda, router_state_data, crate::ID),
            create_account(access_manager_pda, access_manager_data, access_manager::ID),
            create_account(client_pda, client_data, crate::ID),
            create_system_account(relayer),
            create_instructions_sysvar_account_with_caller(crate::ID),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::test_utils::get_router_program_path());

        let checks = vec![Check::err(ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + RouterError::InvalidCounterpartyInfo as u32,
        ))];

        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }

    #[test]
    fn test_add_client_unauthorized_authority() {
        let authority = Pubkey::new_unique();
        let wrong_authority = Pubkey::new_unique();
        let relayer = wrong_authority;
        let light_client_program = Pubkey::new_unique();
        let client_id = "test-client";

        let (router_state_pda, router_state_data) = setup_router_state();
        let (access_manager_pda, access_manager_data) =
            setup_access_manager_with_roles(&[(roles::ID_CUSTOMIZER_ROLE, &[authority])]);
        let (client_pda, _) =
            Pubkey::find_program_address(&[Client::SEED, client_id.as_bytes()], &crate::ID);
        let (client_sequence_pda, _) =
            Pubkey::find_program_address(&[ClientSequence::SEED, client_id.as_bytes()], &crate::ID);

        let instruction_data = crate::instruction::AddClient {
            client_id: client_id.to_string(),
            counterparty_info: CounterpartyInfo {
                client_id: "counterparty-client".to_string(),
                merkle_prefix: vec![vec![0x01, 0x02, 0x03]],
            },
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(wrong_authority, true), // Wrong authority tries to add client
                AccountMeta::new_readonly(router_state_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new(client_pda, false),
                AccountMeta::new(client_sequence_pda, false),
                AccountMeta::new_readonly(relayer, true),
                AccountMeta::new_readonly(light_client_program, false),
                AccountMeta::new_readonly(system_program::ID, false),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_system_account(wrong_authority),
            create_account(router_state_pda, router_state_data, crate::ID),
            create_account(access_manager_pda, access_manager_data, access_manager::ID),
            create_uninitialized_account(client_pda, 0),
            create_uninitialized_account(client_sequence_pda, 0),
            create_system_account(relayer),
            create_program_account(light_client_program),
            create_program_account(system_program::ID),
            create_instructions_sysvar_account_with_caller(crate::ID),
            create_program_account(access_manager::ID),
        ];

        let mollusk = setup_mollusk_with_light_client();

        let checks = vec![Check::err(ProgramError::Custom(
            ANCHOR_ERROR_OFFSET + access_manager::AccessManagerError::Unauthorized as u32,
        ))];

        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }

    #[test]
    fn test_add_client_fake_sysvar_wormhole_attack() {
        let authority = Pubkey::new_unique();
        let relayer = authority;
        let light_client_program = Pubkey::new_unique();
        let client_id = "test-client";

        let (router_state_pda, router_state_data) = setup_router_state();
        let (access_manager_pda, access_manager_data) =
            setup_access_manager_with_roles(&[(roles::ID_CUSTOMIZER_ROLE, &[authority])]);
        let (client_pda, _) =
            Pubkey::find_program_address(&[Client::SEED, client_id.as_bytes()], &crate::ID);
        let (client_sequence_pda, _) =
            Pubkey::find_program_address(&[ClientSequence::SEED, client_id.as_bytes()], &crate::ID);

        let instruction_data = crate::instruction::AddClient {
            client_id: client_id.to_string(),
            counterparty_info: CounterpartyInfo {
                client_id: "counterparty-client".to_string(),
                merkle_prefix: vec![vec![0x01, 0x02, 0x03]],
            },
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(authority, true),
                AccountMeta::new_readonly(router_state_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new(client_pda, false),
                AccountMeta::new(client_sequence_pda, false),
                AccountMeta::new_readonly(relayer, true),
                AccountMeta::new_readonly(light_client_program, false),
                AccountMeta::new_readonly(system_program::ID, false),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
            data: instruction_data.data(),
        };

        // Replace real sysvar with fake one (Wormhole-style attack)
        let (instruction, fake_sysvar_account) = setup_fake_sysvar_attack(instruction, crate::ID);

        let accounts = vec![
            create_system_account(authority),
            create_account(router_state_pda, router_state_data, crate::ID),
            create_account(access_manager_pda, access_manager_data, access_manager::ID),
            create_uninitialized_account(client_pda, 0),
            create_uninitialized_account(client_sequence_pda, 0),
            create_program_account(light_client_program),
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
    fn test_add_client_cpi_rejection() {
        let authority = Pubkey::new_unique();
        let relayer = authority;
        let light_client_program = Pubkey::new_unique();
        let client_id = "test-client";

        let (router_state_pda, router_state_data) = setup_router_state();
        let (access_manager_pda, access_manager_data) =
            setup_access_manager_with_roles(&[(roles::ID_CUSTOMIZER_ROLE, &[authority])]);
        let (client_pda, _) =
            Pubkey::find_program_address(&[Client::SEED, client_id.as_bytes()], &crate::ID);
        let (client_sequence_pda, _) =
            Pubkey::find_program_address(&[ClientSequence::SEED, client_id.as_bytes()], &crate::ID);

        let instruction_data = crate::instruction::AddClient {
            client_id: client_id.to_string(),
            counterparty_info: CounterpartyInfo {
                client_id: "counterparty-client".to_string(),
                merkle_prefix: vec![vec![0x01, 0x02, 0x03]],
            },
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(authority, true),
                AccountMeta::new_readonly(router_state_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new(client_pda, false),
                AccountMeta::new(client_sequence_pda, false),
                AccountMeta::new_readonly(relayer, true),
                AccountMeta::new_readonly(light_client_program, false),
                AccountMeta::new_readonly(system_program::ID, false),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
            data: instruction_data.data(),
        };

        // Simulate CPI call from unauthorized program
        let malicious_program = Pubkey::new_unique();
        let (instruction, cpi_sysvar_account) = setup_cpi_call_test(instruction, malicious_program);

        let accounts = vec![
            create_system_account(authority),
            create_account(router_state_pda, router_state_data, crate::ID),
            create_account(access_manager_pda, access_manager_data, access_manager::ID),
            create_uninitialized_account(client_pda, 0),
            create_uninitialized_account(client_sequence_pda, 0),
            create_system_account(relayer),
            create_program_account(light_client_program),
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

#[cfg(test)]
mod integration_tests {
    use crate::state::{Client, ClientSequence, CounterpartyInfo, RouterState};
    use crate::test_utils::*;
    use anchor_lang::InstructionData;
    use solana_sdk::{
        instruction::{AccountMeta, Instruction},
        pubkey::Pubkey,
        signature::Keypair,
        signer::Signer,
    };

    const CLIENT_ID: &str = "test-client-01";

    fn build_add_client_ix(payer: Pubkey, relayer: Pubkey, client_id: &str) -> Instruction {
        let (router_state_pda, _) =
            Pubkey::find_program_address(&[RouterState::SEED], &crate::ID);
        let (access_manager_pda, _) = Pubkey::find_program_address(
            &[access_manager::state::AccessManager::SEED],
            &access_manager::ID,
        );
        let (client_pda, _) =
            Pubkey::find_program_address(&[Client::SEED, client_id.as_bytes()], &crate::ID);
        let (client_sequence_pda, _) = Pubkey::find_program_address(
            &[ClientSequence::SEED, client_id.as_bytes()],
            &crate::ID,
        );

        Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(router_state_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new(client_pda, false),
                AccountMeta::new(client_sequence_pda, false),
                AccountMeta::new_readonly(relayer, true),
                AccountMeta::new_readonly(Pubkey::new_unique(), false),
                AccountMeta::new_readonly(solana_sdk::system_program::ID, false),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
            data: crate::instruction::AddClient {
                client_id: client_id.to_string(),
                counterparty_info: CounterpartyInfo {
                    client_id: "counterparty-client".to_string(),
                    merkle_prefix: vec![vec![0x01, 0x02, 0x03]],
                },
            }
            .data(),
        }
    }

    fn build_migrate_client_ix(payer: Pubkey, relayer: Pubkey, client_id: &str) -> Instruction {
        let (router_state_pda, _) =
            Pubkey::find_program_address(&[RouterState::SEED], &crate::ID);
        let (access_manager_pda, _) = Pubkey::find_program_address(
            &[access_manager::state::AccessManager::SEED],
            &access_manager::ID,
        );
        let (client_pda, _) =
            Pubkey::find_program_address(&[Client::SEED, client_id.as_bytes()], &crate::ID);

        Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(router_state_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new(client_pda, false),
                AccountMeta::new_readonly(relayer, true),
                AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            ],
            data: crate::instruction::MigrateClient {
                client_id: client_id.to_string(),
                params: super::MigrateClientParams {
                    client_program_id: Some(Pubkey::new_unique()),
                    counterparty_info: None,
                    active: None,
                },
            }
            .data(),
        }
    }

    fn setup_with_client(
        admin: &Pubkey,
        client_id: &str,
        whitelisted: &[Pubkey],
    ) -> solana_program_test::ProgramTest {
        let mut pt = setup_program_test_with_whitelist(admin, whitelisted);

        let (client_pda, client_data) =
            setup_client(client_id, Pubkey::new_unique(), "counterparty-client", true);

        pt.add_account(
            client_pda,
            solana_sdk::account::Account {
                lamports: 1_000_000,
                data: client_data,
                owner: crate::ID,
                executable: false,
                rent_epoch: 0,
            },
        );

        pt
    }

    // ── add_client (require_role → reject_cpi) ──

    #[tokio::test]
    async fn test_add_client_direct_call_succeeds() {
        let admin = Keypair::new();
        let pt = setup_program_test_with_roles_and_whitelist(
            &[(solana_ibc_types::roles::ID_CUSTOMIZER_ROLE, &[admin.pubkey()])],
            &[TEST_CPI_TARGET_ID],
        );
        let (banks_client, payer, recent_blockhash) = pt.start().await;

        let ix = build_add_client_ix(payer.pubkey(), admin.pubkey(), CLIENT_ID);

        let tx = solana_sdk::transaction::Transaction::new_signed_with_payer(
            &[ix],
            Some(&payer.pubkey()),
            &[&payer, &admin],
            recent_blockhash,
        );
        let result = banks_client.process_transaction(tx).await;
        assert!(
            result.is_ok(),
            "Direct call with ID_CUSTOMIZER_ROLE should succeed: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_add_client_without_role_rejected() {
        let admin = Keypair::new();
        let non_admin = Keypair::new();
        let pt = setup_program_test_with_roles_and_whitelist(
            &[(solana_ibc_types::roles::ID_CUSTOMIZER_ROLE, &[admin.pubkey()])],
            &[],
        );
        let (banks_client, payer, recent_blockhash) = pt.start().await;

        let ix = build_add_client_ix(payer.pubkey(), non_admin.pubkey(), CLIENT_ID);

        let tx = solana_sdk::transaction::Transaction::new_signed_with_payer(
            &[ix],
            Some(&payer.pubkey()),
            &[&payer, &non_admin],
            recent_blockhash,
        );
        let err = banks_client.process_transaction(tx).await.unwrap_err();
        assert_eq!(
            pt_extract_custom_error(&err),
            Some(ANCHOR_ERROR_OFFSET + access_manager::AccessManagerError::Unauthorized as u32),
        );
    }

    #[tokio::test]
    async fn test_add_client_cpi_rejected() {
        let admin = Keypair::new();
        let pt = setup_program_test_with_roles_and_whitelist(
            &[(solana_ibc_types::roles::ID_CUSTOMIZER_ROLE, &[admin.pubkey()])],
            &[TEST_CPI_TARGET_ID],
        );
        let (banks_client, payer, recent_blockhash) = pt.start().await;

        let inner_ix = build_add_client_ix(payer.pubkey(), admin.pubkey(), CLIENT_ID);
        let wrapped_ix = pt_wrap_in_test_cpi_proxy(admin.pubkey(), &inner_ix);

        let tx = solana_sdk::transaction::Transaction::new_signed_with_payer(
            &[wrapped_ix],
            Some(&payer.pubkey()),
            &[&payer, &admin],
            recent_blockhash,
        );
        let err = banks_client.process_transaction(tx).await.unwrap_err();
        assert_eq!(
            pt_extract_custom_error(&err),
            Some(ANCHOR_ERROR_OFFSET + access_manager::AccessManagerError::CpiNotAllowed as u32),
        );
    }

    // ── migrate_client (require_admin → whitelist-aware) ──

    #[tokio::test]
    async fn test_migrate_direct_call_by_admin_succeeds() {
        let admin = Keypair::new();
        let pt = setup_with_client(&admin.pubkey(), CLIENT_ID, &[TEST_CPI_TARGET_ID]);
        let (banks_client, payer, recent_blockhash) = pt.start().await;

        let ix = build_migrate_client_ix(payer.pubkey(), admin.pubkey(), CLIENT_ID);

        let tx = solana_sdk::transaction::Transaction::new_signed_with_payer(
            &[ix],
            Some(&payer.pubkey()),
            &[&payer, &admin],
            recent_blockhash,
        );
        let result = banks_client.process_transaction(tx).await;
        assert!(result.is_ok(), "Direct call by admin should succeed");
    }

    #[tokio::test]
    async fn test_migrate_direct_call_by_non_admin_rejected() {
        let admin = Keypair::new();
        let non_admin = Keypair::new();
        let pt = setup_with_client(&admin.pubkey(), CLIENT_ID, &[]);
        let (banks_client, payer, recent_blockhash) = pt.start().await;

        let ix = build_migrate_client_ix(payer.pubkey(), non_admin.pubkey(), CLIENT_ID);

        let tx = solana_sdk::transaction::Transaction::new_signed_with_payer(
            &[ix],
            Some(&payer.pubkey()),
            &[&payer, &non_admin],
            recent_blockhash,
        );
        let err = banks_client.process_transaction(tx).await.unwrap_err();
        assert_eq!(
            pt_extract_custom_error(&err),
            Some(ANCHOR_ERROR_OFFSET + access_manager::AccessManagerError::Unauthorized as u32),
        );
    }

    #[tokio::test]
    async fn test_migrate_whitelisted_cpi_succeeds() {
        let admin = Keypair::new();
        let pt = setup_with_client(&admin.pubkey(), CLIENT_ID, &[TEST_CPI_TARGET_ID]);
        let (banks_client, payer, recent_blockhash) = pt.start().await;

        let inner_ix = build_migrate_client_ix(payer.pubkey(), admin.pubkey(), CLIENT_ID);
        let wrapped_ix = pt_wrap_in_test_cpi_target_proxy(admin.pubkey(), &inner_ix);

        let tx = solana_sdk::transaction::Transaction::new_signed_with_payer(
            &[wrapped_ix],
            Some(&payer.pubkey()),
            &[&payer, &admin],
            recent_blockhash,
        );
        let result = banks_client.process_transaction(tx).await;
        assert!(
            result.is_ok(),
            "Whitelisted CPI should succeed: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_migrate_unauthorized_cpi_rejected() {
        let admin = Keypair::new();
        let pt = setup_with_client(&admin.pubkey(), CLIENT_ID, &[TEST_CPI_TARGET_ID]);
        let (banks_client, payer, recent_blockhash) = pt.start().await;

        let inner_ix = build_migrate_client_ix(payer.pubkey(), admin.pubkey(), CLIENT_ID);
        let wrapped_ix = pt_wrap_in_test_cpi_proxy(admin.pubkey(), &inner_ix);

        let tx = solana_sdk::transaction::Transaction::new_signed_with_payer(
            &[wrapped_ix],
            Some(&payer.pubkey()),
            &[&payer, &admin],
            recent_blockhash,
        );
        let err = banks_client.process_transaction(tx).await.unwrap_err();
        assert_eq!(
            pt_extract_custom_error(&err),
            Some(ANCHOR_ERROR_OFFSET + access_manager::AccessManagerError::CpiNotAllowed as u32),
        );
    }

    #[tokio::test]
    async fn test_migrate_nested_cpi_rejected() {
        let admin = Keypair::new();
        let pt = setup_with_client(&admin.pubkey(), CLIENT_ID, &[TEST_CPI_TARGET_ID]);
        let (banks_client, payer, recent_blockhash) = pt.start().await;

        // Use admin as both authority and relayer (single signer) so the proxy
        // chain can forward signer privilege without "unauthorized signer" errors.
        let inner_ix = build_migrate_client_ix(admin.pubkey(), admin.pubkey(), CLIENT_ID);
        let cpi_target_ix = pt_wrap_in_test_cpi_target_proxy(admin.pubkey(), &inner_ix);
        let nested_ix = pt_wrap_in_test_cpi_proxy(admin.pubkey(), &cpi_target_ix);

        let tx = solana_sdk::transaction::Transaction::new_signed_with_payer(
            &[nested_ix],
            Some(&payer.pubkey()),
            &[&payer, &admin],
            recent_blockhash,
        );
        let err = banks_client.process_transaction(tx).await.unwrap_err();
        assert_eq!(
            pt_extract_custom_error(&err),
            Some(ANCHOR_ERROR_OFFSET + access_manager::AccessManagerError::CpiNotAllowed as u32),
        );
    }
}
