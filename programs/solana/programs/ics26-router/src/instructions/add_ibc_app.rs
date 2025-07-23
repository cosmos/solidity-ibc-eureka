use crate::errors::RouterError;
use crate::state::{IBCApp, RouterState, IBC_APP_SEED, ROUTER_STATE_SEED};
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
        space = 8 + IBCApp::INIT_SPACE,
        seeds = [IBC_APP_SEED, port_id.as_bytes()],
        bump
    )]
    pub ibc_app: Account<'info, IBCApp>,

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
    let ibc_app = &mut ctx.accounts.ibc_app;

    require!(
        ctx.accounts.authority.key() == router_state.authority,
        RouterError::UnauthorizedSender
    );

    require!(!port_id.is_empty(), RouterError::InvalidPortIdentifier);

    ibc_app.port_id = port_id;
    ibc_app.app_program_id = ctx.accounts.app_program.key();
    ibc_app.authority = ctx.accounts.authority.key();

    emit!(IBCAppAdded {
        port_id: ibc_app.port_id.clone(),
        app_program_id: ibc_app.app_program_id,
    });

    Ok(())
}

#[event]
pub struct IBCAppAdded {
    pub port_id: String,
    pub app_program_id: Pubkey,
}
