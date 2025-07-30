use crate::errors::RouterError;
use crate::state::{Client, CounterpartyInfo, RouterState, CLIENT_SEED, ROUTER_STATE_SEED};
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
        constraint = router_state.initialized @ RouterError::RouterNotInitialized,
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
        constraint = router_state.initialized @ RouterError::RouterNotInitialized,
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
        !client_id.is_empty() && client_id.len() <= 64,
        RouterError::InvalidClientId
    );

    // The program ID validation happens during verification when we check
    // that the light client program matches what's stored in the client registry

    require!(
        !counterparty_info.client_id.is_empty(),
        RouterError::InvalidCounterpartyInfo
    );
    require!(
        !counterparty_info.connection_id.is_empty(),
        RouterError::InvalidCounterpartyInfo
    );

    client.client_id = client_id;
    client.client_program_id = light_client_program.key();
    client.counterparty_info = counterparty_info;
    client.authority = ctx.accounts.authority.key();
    client.active = true;

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
    use anchor_lang::{AnchorDeserialize, AnchorSerialize, Discriminator, InstructionData};
    use mollusk_svm::result::Check;
    use mollusk_svm::Mollusk;
    use solana_sdk::account::Account;
    use solana_sdk::instruction::{AccountMeta, Instruction};
    use solana_sdk::pubkey::Pubkey;
    use solana_sdk::{native_loader, system_program};

    fn create_account_data<T: Discriminator + AnchorSerialize>(account: &T) -> Vec<u8> {
        let mut data = T::DISCRIMINATOR.to_vec();
        account.serialize(&mut data).unwrap();
        data
    }

    fn setup_router_state(authority: Pubkey) -> (Pubkey, Vec<u8>) {
        let (router_state_pda, _) = Pubkey::find_program_address(&[ROUTER_STATE_SEED], &crate::ID);

        let router_state = RouterState {
            authority,
            initialized: true,
        };

        let router_state_data = create_account_data(&router_state);

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

        let client = Client {
            client_id: client_id.to_string(),
            client_program_id: light_client_program,
            counterparty_info: CounterpartyInfo {
                client_id: "counterparty-client".to_string(),
                connection_id: "connection-0".to_string(),
                merkle_prefix: vec![0x01, 0x02, 0x03],
            },
            authority,
            active,
        };

        let client_data = create_account_data(&client);

        (client_pda, client_data)
    }

    #[test]
    fn test_add_client_happy_path() {
        let authority = Pubkey::new_unique();
        let relayer = authority; // Relayer must be the same as authority
        let light_client_program = Pubkey::new_unique();
        let client_id = "test-client-01";

        let (router_state_pda, router_state_data) = setup_router_state(authority);
        let (client_pda, _) =
            Pubkey::find_program_address(&[CLIENT_SEED, client_id.as_bytes()], &crate::ID);

        let counterparty_info = CounterpartyInfo {
            client_id: "counterparty-client".to_string(),
            connection_id: "connection-0".to_string(),
            merkle_prefix: vec![0x01, 0x02, 0x03],
        };

        let instruction_data = crate::instruction::AddClient {
            client_id: client_id.to_string(),
            counterparty_info: counterparty_info.clone(),
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(authority, true),
                AccountMeta::new_readonly(router_state_pda, false),
                AccountMeta::new(client_pda, false),
                AccountMeta::new_readonly(relayer, true),
                AccountMeta::new_readonly(light_client_program, false),
                AccountMeta::new_readonly(system_program::ID, false),
            ],
            data: instruction_data.data(),
        };

        let authority_lamports = 10_000_000_000;
        let accounts = vec![
            (
                authority,
                Account {
                    lamports: authority_lamports,
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

        let mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());

        let checks = vec![
            Check::success(),
            Check::account(&client_pda).owner(&crate::ID).build(),
        ];

        let result = mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);

        let authority_account = result
            .resulting_accounts
            .iter()
            .find(|(pubkey, _)| pubkey == &authority)
            .map(|(_, account)| account)
            .expect("Authority account not found");

        assert!(
            authority_account.lamports < authority_lamports,
            "Authority should have paid for account creation"
        );

        let client_account = result
            .resulting_accounts
            .iter()
            .find(|(pubkey, _)| pubkey == &client_pda)
            .map(|(_, account)| account)
            .expect("Client account not found");

        assert!(
            client_account.lamports > 0,
            "Client account should be rent-exempt"
        );

        let mut data_slice = &client_account.data[8..];
        let deserialized_client: Client =
            Client::deserialize(&mut data_slice).expect("Failed to deserialize client");

        assert_eq!(deserialized_client.client_id, client_id);
        assert_eq!(deserialized_client.client_program_id, light_client_program);
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

        let mut data_slice = &client_account.data[8..];
        let deserialized_client: Client =
            Client::deserialize(&mut data_slice).expect("Failed to deserialize client");

        assert!(!deserialized_client.active, "Client should be deactivated");
        assert_eq!(deserialized_client.client_id, client_id);
        assert_eq!(deserialized_client.client_program_id, light_client_program);
        assert_eq!(deserialized_client.authority, authority);
    }
}
