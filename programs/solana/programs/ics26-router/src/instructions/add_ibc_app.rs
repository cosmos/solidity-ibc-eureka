use crate::errors::RouterError;
use crate::state::{Port, RouterState, PORT_SEED, ROUTER_STATE_SEED};
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
        space = 8 + Port::INIT_SPACE,
        seeds = [PORT_SEED, port_id.as_bytes()],
        bump
    )]
    pub port: Account<'info, Port>,

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
    let port = &mut ctx.accounts.port;

    require!(
        ctx.accounts.authority.key() == router_state.authority,
        RouterError::UnauthorizedSender
    );

    require!(!port_id.is_empty(), RouterError::InvalidPortIdentifier);

    port.port_id = port_id;
    port.app_program_id = ctx.accounts.app_program.key();
    port.authority = ctx.accounts.authority.key();

    emit!(PortAdded {
        port_id: port.port_id.clone(),
        app_program_id: port.app_program_id,
    });

    Ok(())
}

#[event]
pub struct PortAdded {
    pub port_id: String,
    pub app_program_id: Pubkey,
}
