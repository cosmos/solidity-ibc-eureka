use crate::state::*;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = payer,
        space = 8 + DummyIbcAppState::INIT_SPACE,
        seeds = [APP_STATE_SEED],
        bump
    )]
    pub app_state: Account<'info, DummyIbcAppState>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn initialize(ctx: Context<Initialize>, authority: Pubkey) -> Result<()> {
    let app_state = &mut ctx.accounts.app_state;

    app_state.authority = authority;
    app_state.packets_received = 0;
    app_state.packets_acknowledged = 0;
    app_state.packets_timed_out = 0;
    app_state.packets_sent = 0;

    msg!("Dummy IBC App initialized with authority: {}", authority);

    Ok(())
}
