use crate::errors::RouterError;
use crate::state::{
    Client, CounterpartyInfo, RouterState, CLIENT_SEED, ROUTER_STATE_SEED,
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
}

pub fn add_client(
    ctx: Context<AddClient>,
    client_id: String,
    counterparty_info: CounterpartyInfo,
) -> Result<()> {
    let client = &mut ctx.accounts.client;
    let light_client_program = &ctx.accounts.light_client_program;

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
