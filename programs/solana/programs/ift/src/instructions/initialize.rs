use anchor_lang::prelude::*;

use crate::constants::*;
use crate::events::IFTInitialized;
use crate::state::{AccountVersion, IFTAppState};

#[derive(Accounts)]
pub struct Initialize<'info> {
    /// Global IFT app state PDA (to be created, singleton)
    #[account(
        init,
        payer = payer,
        space = 8 + IFTAppState::INIT_SPACE,
        seeds = [IFT_APP_STATE_SEED],
        bump
    )]
    pub app_state: Account<'info, IFTAppState>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn initialize(ctx: Context<Initialize>, admin: Pubkey, gmp_program: Pubkey) -> Result<()> {
    let app_state = &mut ctx.accounts.app_state;
    app_state.version = AccountVersion::V1;
    app_state.bump = ctx.bumps.app_state;
    app_state.admin = admin;
    app_state.gmp_program = gmp_program;

    let clock = Clock::get()?;
    emit!(IFTInitialized {
        admin,
        gmp_program,
        timestamp: clock.unix_timestamp,
    });

    Ok(())
}
