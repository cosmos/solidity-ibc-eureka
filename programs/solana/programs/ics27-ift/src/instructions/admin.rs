use anchor_lang::prelude::*;

use crate::constants::*;
use crate::errors::IFTError;
use crate::events::{IFTAppPaused, IFTAppUnpaused};
use crate::state::IFTAppState;

#[derive(Accounts)]
pub struct PauseApp<'info> {
    #[account(
        mut,
        seeds = [IFT_APP_STATE_SEED, app_state.mint.as_ref()],
        bump = app_state.bump,
        constraint = !app_state.paused @ IFTError::AppPaused
    )]
    pub app_state: Account<'info, IFTAppState>,

    /// Admin with pause role
    /// TODO: Validate role via access manager CPI
    pub admin: Signer<'info>,
}

pub fn pause_app(ctx: Context<PauseApp>) -> Result<()> {
    // TODO: Validate admin has required role via access manager CPI

    ctx.accounts.app_state.paused = true;

    let clock = Clock::get()?;
    emit!(IFTAppPaused {
        mint: ctx.accounts.app_state.mint,
        admin: ctx.accounts.admin.key(),
        timestamp: clock.unix_timestamp,
    });

    Ok(())
}

#[derive(Accounts)]
pub struct UnpauseApp<'info> {
    #[account(
        mut,
        seeds = [IFT_APP_STATE_SEED, app_state.mint.as_ref()],
        bump = app_state.bump,
        constraint = app_state.paused @ IFTError::AppPaused
    )]
    pub app_state: Account<'info, IFTAppState>,

    /// Admin with unpause role
    /// TODO: Validate role via access manager CPI
    pub admin: Signer<'info>,
}

pub fn unpause_app(ctx: Context<UnpauseApp>) -> Result<()> {
    // TODO: Validate admin has required role via access manager CPI

    ctx.accounts.app_state.paused = false;

    let clock = Clock::get()?;
    emit!(IFTAppUnpaused {
        mint: ctx.accounts.app_state.mint,
        admin: ctx.accounts.admin.key(),
        timestamp: clock.unix_timestamp,
    });

    Ok(())
}

#[derive(Accounts)]
pub struct SetAccessManager<'info> {
    #[account(
        mut,
        seeds = [IFT_APP_STATE_SEED, app_state.mint.as_ref()],
        bump = app_state.bump
    )]
    pub app_state: Account<'info, IFTAppState>,

    /// Admin with admin role
    /// TODO: Validate role via access manager CPI
    pub admin: Signer<'info>,
}

pub fn set_access_manager(ctx: Context<SetAccessManager>, new_access_manager: Pubkey) -> Result<()> {
    // TODO: Validate admin has required role via access manager CPI

    ctx.accounts.app_state.access_manager = new_access_manager;

    Ok(())
}
