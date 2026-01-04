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

    /// CHECK: Validated via seeds constraint using stored `access_manager` program ID
    #[account(
        seeds = [access_manager::state::AccessManager::SEED],
        bump,
        seeds::program = app_state.access_manager
    )]
    pub access_manager: AccountInfo<'info>,

    /// Authority with admin role
    pub authority: Signer<'info>,

    /// Instructions sysvar for CPI validation
    /// CHECK: Address constraint verifies this is the instructions sysvar
    #[account(address = anchor_lang::solana_program::sysvar::instructions::ID)]
    pub instructions_sysvar: AccountInfo<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn remove_ift_bridge(ctx: Context<RemoveIFTBridge>) -> Result<()> {
    access_manager::require_role(
        &ctx.accounts.access_manager,
        solana_ibc_types::roles::ADMIN_ROLE,
        &ctx.accounts.authority,
        &ctx.accounts.instructions_sysvar,
        &crate::ID,
    )?;

    let client_id = ctx.accounts.ift_bridge.client_id.clone();

    let clock = Clock::get()?;
    emit!(IFTBridgeRemoved {
        mint: ctx.accounts.app_state.mint,
        client_id,
        timestamp: clock.unix_timestamp,
    });

    // Bridge account is closed via Anchor's close constraint
    Ok(())
}

#[cfg(test)]
mod tests;
