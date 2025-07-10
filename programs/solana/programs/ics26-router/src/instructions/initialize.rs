use crate::errors::IbcRouterError;
use crate::state::{RouterState, ROUTER_STATE_SEED};
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = payer,
        space = 8 + 32 + 1, // discriminator + pubkey + bool
        seeds = [ROUTER_STATE_SEED],
        bump
    )]
    pub router_state: Account<'info, RouterState>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn initialize(ctx: Context<Initialize>, authority: Pubkey) -> Result<()> {
    let router_state = &mut ctx.accounts.router_state;

    require!(
        !router_state.initialized,
        IbcRouterError::AlreadyInitialized
    );

    router_state.authority = authority;
    router_state.initialized = true;

    Ok(())
}

