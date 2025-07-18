use crate::errors::RouterError;
use crate::state::{PortRegistry, RouterState, PORT_REGISTRY_SEED, ROUTER_STATE_SEED};
use anchor_lang::prelude::*;

#[derive(Accounts)]
#[instruction(port_id: String)]
pub struct AddIbcApp<'info> {
    #[account(
        seeds = [ROUTER_STATE_SEED],
        bump
    )]
    pub router_state: Account<'info, RouterState>,

    #[account(
        init,
        payer = payer,
        space = 8 + 4 + port_id.len() + 32 + 32, // discriminator + string len + string + 2 pubkeys
        seeds = [PORT_REGISTRY_SEED, port_id.as_bytes()],
        bump
    )]
    pub port_registry: Account<'info, PortRegistry>,

    /// The IBC application program to register
    /// CHECK: This is the program ID of the IBC app
    pub app_program: UncheckedAccount<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub authority: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn add_ibc_app(ctx: Context<AddIbcApp>, port_id: String) -> Result<()> {
    let router_state = &ctx.accounts.router_state;
    let port_registry = &mut ctx.accounts.port_registry;

    require!(
        ctx.accounts.authority.key() == router_state.authority,
        RouterError::UnauthorizedSender
    );

    require!(!port_id.is_empty(), RouterError::InvalidPortIdentifier);

    port_registry.port_id = port_id;
    port_registry.app_program_id = ctx.accounts.app_program.key();
    port_registry.authority = ctx.accounts.authority.key();

    emit!(PortAdded {
        port_id: port_registry.port_id.clone(),
        app_program_id: port_registry.app_program_id,
    });

    Ok(())
}

#[event]
pub struct PortAdded {
    pub port_id: String,
    pub app_program_id: Pubkey,
}
