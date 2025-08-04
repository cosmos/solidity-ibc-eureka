use crate::errors::RouterError;
use crate::state::{
    Client, ClientSequence, CounterpartyInfo, RouterState, CLIENT_SEED, CLIENT_SEQUENCE_SEED,
    ROUTER_STATE_SEED,
};
use anchor_lang::prelude::*;

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
        !counterparty_info.connection_id.is_empty(),
        RouterError::InvalidCounterpartyInfo
    );
    require!(
        !counterparty_info.merkle_prefix.is_empty(),
        RouterError::InvalidCounterpartyInfo
    );

    client.client_id = client_id;
    client.client_program_id = light_client_program.key();
    client.counterparty_info = counterparty_info;
    client.authority = ctx.accounts.authority.key();
    client.active = true;

    // ClientSequence is automatically initialized with Default trait (next_sequence_send = 0)
    // The first packet will use sequence 1 (incremented before use)

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

#[event]
pub struct ClientAddedEvent {
    pub client_id: String,
    pub client_program_id: Pubkey,
    pub authority: Pubkey,
}

#[event]
pub struct ClientStatusUpdatedEvent {
    pub client_id: String,
    pub active: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::RouterState;
    use anchor_lang::{AnchorDeserialize, InstructionData};
    use mollusk_svm::result::Check;
    use mollusk_svm::Mollusk;
    use solana_sdk::account::Account;
    use solana_sdk::instruction::{AccountMeta, Instruction};
    use solana_sdk::pubkey::Pubkey;
    use solana_sdk::{native_loader, system_program};

    fn create_account_data(
        account_name: &str,
        init_space: usize,
        serialize_fn: impl FnOnce(&mut [u8]),
    ) -> Vec<u8> {
        let mut data = vec![0u8; 8 + init_space];

        // Write discriminator
        let discriminator: [u8; 8] =
            anchor_lang::solana_program::hash::hash(format!("account:{account_name}").as_bytes())
                .to_bytes()[..8]
                .try_into()
                .unwrap();
        data[0..8].copy_from_slice(&discriminator);

        // Serialize account data
        serialize_fn(&mut data[8..]);

        data
    }

    fn serialize_string(data: &mut [u8], offset: &mut usize, value: &str) {
        let bytes = value.as_bytes();
        let len = bytes.len() as u32;
        data[*offset..*offset + 4].copy_from_slice(&len.to_le_bytes());
        *offset += 4;
        data[*offset..*offset + bytes.len()].copy_from_slice(bytes);
        *offset += bytes.len();
    }

    fn serialize_vec_u8(data: &mut [u8], offset: &mut usize, value: &[u8]) {
        let len = value.len() as u32;
        data[*offset..*offset + 4].copy_from_slice(&len.to_le_bytes());
        *offset += 4;
        data[*offset..*offset + value.len()].copy_from_slice(value);
        *offset += value.len();
    }

    fn setup_router_state(authority: Pubkey) -> (Pubkey, Vec<u8>) {
        let (router_state_pda, _) = Pubkey::find_program_address(&[ROUTER_STATE_SEED], &crate::ID);

        let router_state_data =
            create_account_data("RouterState", RouterState::INIT_SPACE, |data| {
                data[0..32].copy_from_slice(authority.as_ref()); // authority: Pubkey
            });

        (router_state_pda, router_state_data)
    }

    fn setup_client(
        client_id: &str,
        light_client_program: Pubkey,
        authority: Pubkey,
        active: bool,
    ) -> (Pubkey, Vec<u8>) {
        let (client_pda, _) =
            Pubkey::find_program_address(&[CLIENT_SEED, client_id.as_bytes()], &crate::ID);

        let client_data = create_account_data("Client", Client::INIT_SPACE, |data| {
            let mut offset = 0;

            // client_id: String
            serialize_string(data, &mut offset, client_id);

            // client_program_id: Pubkey (32 bytes)
            data[offset..offset + 32].copy_from_slice(light_client_program.as_ref());
            offset += 32;

            // counterparty_info.client_id: String
            serialize_string(data, &mut offset, "counterparty-client");

            // counterparty_info.connection_id: String
            serialize_string(data, &mut offset, "connection-0");

            // counterparty_info.merkle_prefix: Vec<u8>
            serialize_vec_u8(data, &mut offset, &[0x01, 0x02, 0x03]);

            // authority: Pubkey (32 bytes)
            data[offset..offset + 32].copy_from_slice(authority.as_ref());
            offset += 32;

            // active: bool (1 byte)
            data[offset] = u8::from(active);
        });

        (client_pda, client_data)
    }

    #[test]
    fn test_add_client_happy_path() {
        let client_id = "test-client-01";
        let counterparty_info = CounterpartyInfo {
            client_id: "counterparty-client".to_string(),
            connection_id: "connection-0".to_string(),
            merkle_prefix: vec![0x01, 0x02, 0x03],
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

        let mut data_slice = &client_account.data[8..];
        let deserialized_client: Client =
            Client::deserialize(&mut data_slice).expect("Failed to deserialize client");

        assert_eq!(deserialized_client.client_id, client_id);
        assert_eq!(deserialized_client.authority, authority);
        assert!(deserialized_client.active);
        assert_eq!(
            deserialized_client.counterparty_info.client_id,
            counterparty_info.client_id
        );
        assert_eq!(
            deserialized_client.counterparty_info.connection_id,
            counterparty_info.connection_id
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

        let mut data_slice = &client_sequence_account.data[8..];
        let deserialized_sequence: ClientSequence = ClientSequence::deserialize(&mut data_slice)
            .expect("Failed to deserialize client sequence");

        assert_eq!(
            deserialized_sequence.next_sequence_send, 0,
            "Sequence should be initialized to 0"
        );
    }

    /// Anchor error code offset
    const ANCHOR_ERROR_OFFSET: u32 = 6000;

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
                connection_id: "connection-0".to_string(),
                merkle_prefix: vec![0x01, 0x02, 0x03],
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
            (
                authority,
                Account {
                    lamports: 10_000_000_000,
                    data: vec![],
                    owner: system_program::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            (
                router_state_pda,
                Account {
                    lamports: 1_000_000,
                    data: router_state_data,
                    owner: crate::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            (
                client_pda,
                Account {
                    lamports: 0,
                    data: vec![],
                    owner: system_program::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            (
                client_sequence_pda,
                Account {
                    lamports: 0,
                    data: vec![],
                    owner: system_program::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            (
                light_client_program,
                Account {
                    lamports: 0,
                    data: vec![],
                    owner: native_loader::ID,
                    executable: true,
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

        let mollusk = Mollusk::new(&crate::ID, crate::ROUTER_PROGRAM_PATH);

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
    fn test_add_client_invalid_counterparty_info_empty_connection() {
        test_add_client(AddClientTestConfig::with_counterparty_info(
            "test-client-03",
            CounterpartyInfo {
                client_id: "counterparty-client".to_string(),
                connection_id: "".to_string(), // Invalid: empty
                merkle_prefix: vec![0x01, 0x02, 0x03],
            },
            RouterError::InvalidCounterpartyInfo,
        ));
    }

    #[test]
    fn test_add_client_invalid_counterparty_info_empty_merkle_prefix() {
        test_add_client(AddClientTestConfig::with_counterparty_info(
            "test-client-04",
            CounterpartyInfo {
                client_id: "counterparty-client".to_string(),
                connection_id: "connection-0".to_string(),
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
        let (client_pda, client_data) =
            setup_client(client_id, light_client_program, authority, true);

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
            (
                authority,
                Account {
                    lamports: 10_000_000_000,
                    data: vec![],
                    owner: system_program::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            (
                router_state_pda,
                Account {
                    lamports: 1_000_000,
                    data: router_state_data,
                    owner: crate::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            (
                client_pda,
                Account {
                    lamports: 1_000_000,
                    data: client_data,
                    owner: crate::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::ROUTER_PROGRAM_PATH);

        let checks = vec![Check::success()];

        let result = mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);

        // Verify client was updated
        let client_account = result
            .resulting_accounts
            .iter()
            .find(|(pubkey, _)| pubkey == &client_pda)
            .map(|(_, account)| account)
            .expect("Client account not found");

        let mut data_slice = &client_account.data[8..];
        let deserialized_client: Client =
            Client::deserialize(&mut data_slice).expect("Failed to deserialize client");

        assert!(!deserialized_client.active, "Client should be deactivated");
        assert_eq!(deserialized_client.client_id, client_id);
        assert_eq!(deserialized_client.client_program_id, light_client_program);
        assert_eq!(deserialized_client.authority, authority);
    }
}
