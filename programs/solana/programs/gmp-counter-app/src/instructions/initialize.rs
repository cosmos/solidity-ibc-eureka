use crate::state::*;
use anchor_lang::prelude::*;

/// Initialize the counter app
#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = payer,
        space = 8 + CounterAppState::INIT_SPACE,
        seeds = [CounterAppState::SEED],
        bump
    )]
    pub app_state: Account<'info, CounterAppState>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn initialize(ctx: Context<Initialize>, authority: Pubkey) -> Result<()> {
    let app_state = &mut ctx.accounts.app_state;

    app_state.authority = authority;
    app_state.total_counters = 0;
    app_state.total_gmp_calls = 0;
    app_state.bump = ctx.bumps.app_state;

    msg!("Counter App initialized with authority: {}", authority);
    Ok(())
}
