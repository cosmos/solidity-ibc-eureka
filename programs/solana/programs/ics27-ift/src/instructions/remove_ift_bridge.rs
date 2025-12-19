use anchor_lang::prelude::*;

use crate::constants::*;
use crate::errors::IFTError;
use crate::events::IFTBridgeRemoved;
use crate::state::{IFTAppState, IFTBridge};

#[derive(Accounts)]
pub struct RemoveIFTBridge<'info> {
    #[account(
        mut,
        seeds = [IFT_APP_STATE_SEED, app_state.mint.as_ref()],
        bump = app_state.bump,
        constraint = !app_state.paused @ IFTError::AppPaused
    )]
    pub app_state: Account<'info, IFTAppState>,

    /// IFT bridge to remove (close and refund rent)
    #[account(
        mut,
        close = payer,
        seeds = [IFT_BRIDGE_SEED, app_state.mint.as_ref(), ift_bridge.client_id.as_bytes()],
        bump = ift_bridge.bump,
        constraint = ift_bridge.mint == app_state.mint @ IFTError::BridgeNotFound
    )]
    pub ift_bridge: Account<'info, IFTBridge>,

    /// Authority with admin role
    /// TODO: Validate role via access manager CPI
    pub authority: Signer<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn remove_ift_bridge(ctx: Context<RemoveIFTBridge>) -> Result<()> {
    // TODO: Validate authority has required role via access manager CPI

    let client_id = ctx.accounts.ift_bridge.client_id.clone();

    ctx.accounts.app_state.total_bridges = ctx
        .accounts
        .app_state
        .total_bridges
        .saturating_sub(1);

    let clock = Clock::get()?;
    emit!(IFTBridgeRemoved {
        mint: ctx.accounts.app_state.mint,
        client_id,
        timestamp: clock.unix_timestamp,
    });

    // Bridge account is closed via Anchor's close constraint
    Ok(())
}
