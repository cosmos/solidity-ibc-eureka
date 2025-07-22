use anchor_lang::prelude::*;
use crate::state::{
    ClientRegistry, ClientType, CounterpartyInfo, RouterState,
    CLIENT_REGISTRY_SEED, ROUTER_STATE_SEED
};
use crate::errors::RouterError;

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
        space = 8 + ClientRegistry::INIT_SPACE,
        seeds = [CLIENT_REGISTRY_SEED, client_id.as_bytes()],
        bump,
    )]
    pub client_registry: Account<'info, ClientRegistry>,

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
        seeds = [CLIENT_REGISTRY_SEED, client_id.as_bytes()],
        bump,
        constraint = client_registry.authority == authority.key() @ RouterError::UnauthorizedAuthority,
    )]
    pub client_registry: Account<'info, ClientRegistry>,
}

pub fn add_client(
    ctx: Context<AddClient>,
    client_id: String,
    client_type: ClientType,
    counterparty_info: CounterpartyInfo,
) -> Result<()> {
    let client_registry = &mut ctx.accounts.client_registry;
    let light_client_program = &ctx.accounts.light_client_program;

    require!(
        !client_id.is_empty() && client_id.len() <= 64,
        RouterError::InvalidClientId
    );

    let expected_program_id = match client_type {
        ClientType::ICS07Tendermint => {
            // Known ICS07 Tendermint program ID
            "8wQAC7oWLTxExhR49jYAzXZB39mu7WVVvkWJGgAMMjpV"
                .parse::<Pubkey>()
                .map_err(|_| RouterError::InvalidLightClientProgram)?
        }
    };

    require!(
        light_client_program.key() == expected_program_id,
        RouterError::InvalidLightClientProgram
    );

    require!(
        !counterparty_info.client_id.is_empty(),
        RouterError::InvalidCounterpartyInfo
    );
    require!(
        !counterparty_info.connection_id.is_empty(),
        RouterError::InvalidCounterpartyInfo
    );

    client_registry.client_id = client_id;
    client_registry.client_program_id = light_client_program.key();
    client_registry.client_type = client_type;
    client_registry.counterparty_info = counterparty_info;
    client_registry.authority = ctx.accounts.authority.key();
    client_registry.active = true;

    emit!(ClientAddedEvent {
        client_id: client_registry.client_id.clone(),
        client_type: client_registry.client_type.clone(),
        client_program_id: client_registry.client_program_id,
        authority: client_registry.authority,
    });

    Ok(())
}

pub fn update_client(
    ctx: Context<UpdateClient>,
    _client_id: String,
    active: bool,
) -> Result<()> {
    let client_registry = &mut ctx.accounts.client_registry;

    client_registry.active = active;

    emit!(ClientStatusUpdatedEvent {
        client_id: client_registry.client_id.clone(),
        active,
    });

    Ok(())
}

#[event]
pub struct ClientAddedEvent {
    pub client_id: String,
    pub client_type: ClientType,
    pub client_program_id: Pubkey,
    pub authority: Pubkey,
}

#[event]
pub struct ClientStatusUpdatedEvent {
    pub client_id: String,
    pub active: bool,
}
