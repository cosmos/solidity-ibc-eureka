use crate::errors::RouterError;
use crate::state::{
    AccountVersion, Client, ClientSequence, CounterpartyInfo, RouterState, CLIENT_SEED,
    CLIENT_SEQUENCE_SEED, ROUTER_STATE_SEED,
};
use anchor_lang::prelude::*;
use solana_ibc_types::events::{ClientAddedEvent, ClientStatusUpdatedEvent};

#[derive(Accounts)]
#[instruction(client_id: String)]
pub struct AddClient<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(
        seeds = [ROUTER_STATE_SEED],
        bump,
        constraint = router_state.authority == authority.key() @ RouterError::UnauthorizedAuthority,
    )]
    pub router_state: Account<'info, RouterState>,

    #[account(
        init,
        payer = authority,
        space = 8 + Client::INIT_SPACE,
        seeds = [CLIENT_SEED, client_id.as_bytes()],
        bump,
    )]
    pub client: Account<'info, Client>,

    #[account(
        init,
        payer = authority,
        space = 8 + ClientSequence::INIT_SPACE,
        seeds = [CLIENT_SEQUENCE_SEED, client_id.as_bytes()],
        bump,
    )]
    pub client_sequence: Account<'info, ClientSequence>,

    pub relayer: Signer<'info>,

    /// CHECK: Light client program ID validation happens in instruction
    pub light_client_program: AccountInfo<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(client_id: String)]
pub struct UpdateClient<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(
        seeds = [ROUTER_STATE_SEED],
        bump,
        constraint = router_state.authority == authority.key() @ RouterError::UnauthorizedAuthority,
    )]
    pub router_state: Account<'info, RouterState>,

    #[account(
        mut,
        seeds = [CLIENT_SEED, client_id.as_bytes()],
        bump,
        constraint = client.authority == authority.key() @ RouterError::UnauthorizedAuthority,
    )]
    pub client: Account<'info, Client>,

    pub relayer: Signer<'info>,
}

pub fn add_client(
    ctx: Context<AddClient>,
    client_id: String,
    counterparty_info: CounterpartyInfo,
) -> Result<()> {
    let client = &mut ctx.accounts.client;
    let light_client_program = &ctx.accounts.light_client_program;
    let router_state = &ctx.accounts.router_state;

    require!(
        ctx.accounts.relayer.key() == router_state.authority,
        RouterError::UnauthorizedSender
    );

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
    client.authority = ctx.accounts.authority.key();
    client.active = true;
    client._reserved = [0u8; 256];

    // Initialize client sequence to start from 1 (IBC sequences start from 1, not 0)
    let client_sequence = &mut ctx.accounts.client_sequence;
    client_sequence.next_sequence_send = 1;

    emit!(ClientAddedEvent {
        client_id: client.client_id.clone(),
        client_program_id: client.client_program_id,
        authority: client.authority,
    });

    Ok(())
}

pub fn update_client(ctx: Context<UpdateClient>, _client_id: String, active: bool) -> Result<()> {
    let client = &mut ctx.accounts.client;
    let router_state = &ctx.accounts.router_state;

    require!(
        ctx.accounts.relayer.key() == router_state.authority,
        RouterError::UnauthorizedSender
    );

    client.active = active;

    emit!(ClientStatusUpdatedEvent {
        client_id: client.client_id.clone(),
        active,
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

        let (router_state_pda, router_state_data) = setup_router_state(authority);
        let (client_pda, _) =
            Pubkey::find_program_address(&[CLIENT_SEED, config.client_id.as_bytes()], &crate::ID);
        let (client_sequence_pda, _) = Pubkey::find_program_address(
            &[CLIENT_SEQUENCE_SEED, config.client_id.as_bytes()],
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
                AccountMeta::new(client_pda, false),
                AccountMeta::new(client_sequence_pda, false),
                AccountMeta::new_readonly(relayer, true),
                AccountMeta::new_readonly(light_client_program, false),
                AccountMeta::new_readonly(system_program::ID, false),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_system_account(authority),
            create_account(router_state_pda, router_state_data, crate::ID),
            create_uninitialized_account(client_pda, 0),
            create_uninitialized_account(client_sequence_pda, 0),
            create_program_account(light_client_program),
            create_program_account(system_program::ID),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());

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
            Pubkey::find_program_address(&[CLIENT_SEED, client_id.as_bytes()], &crate::ID);
        let (client_sequence_pda, _) =
            Pubkey::find_program_address(&[CLIENT_SEQUENCE_SEED, client_id.as_bytes()], &crate::ID);

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
        assert_eq!(deserialized_client.authority, authority);
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
    fn test_update_client_happy_path() {
        let authority = Pubkey::new_unique();
        let relayer = authority; // Same as authority for this test
        let light_client_program = Pubkey::new_unique();
        let client_id = "test-client-02";

        let (router_state_pda, router_state_data) = setup_router_state(authority);
        let (client_pda, client_data) = setup_client(
            client_id,
            authority,
            light_client_program,
            "counterparty-client",
            true,
        );

        let instruction_data = crate::instruction::UpdateClient {
            client_id: client_id.to_string(),
            active: false, // Deactivate the client
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(authority, true),
                AccountMeta::new_readonly(router_state_pda, false),
                AccountMeta::new(client_pda, false),
                AccountMeta::new_readonly(relayer, true),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            create_system_account(authority),
            create_account(router_state_pda, router_state_data, crate::ID),
            create_account(client_pda, client_data, crate::ID),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());

        let checks = vec![Check::success()];

        let result = mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);

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
        assert_eq!(deserialized_client.authority, authority);
    }
}
