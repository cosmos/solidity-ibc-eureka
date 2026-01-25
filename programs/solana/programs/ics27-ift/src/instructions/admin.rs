use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token};

use crate::constants::*;
use crate::events::MintAuthorityRevoked;
use crate::state::IFTAppState;

#[derive(Accounts)]
pub struct SetAccessManager<'info> {
    #[account(
        mut,
        seeds = [IFT_APP_STATE_SEED, app_state.mint.as_ref()],
        bump = app_state.bump
    )]
    pub app_state: Account<'info, IFTAppState>,

    /// CHECK: Validated via seeds constraint using stored `access_manager` program ID
    #[account(
        seeds = [access_manager::state::AccessManager::SEED],
        bump,
        seeds::program = app_state.access_manager
    )]
    pub access_manager: AccountInfo<'info>,

    /// Admin with admin role
    pub admin: Signer<'info>,

    /// Instructions sysvar for CPI validation
    /// CHECK: Address constraint verifies this is the instructions sysvar
    #[account(address = anchor_lang::solana_program::sysvar::instructions::ID)]
    pub instructions_sysvar: AccountInfo<'info>,
}

pub fn set_access_manager(
    ctx: Context<SetAccessManager>,
    new_access_manager: Pubkey,
) -> Result<()> {
    access_manager::require_role(
        &ctx.accounts.access_manager,
        solana_ibc_types::roles::ADMIN_ROLE,
        &ctx.accounts.admin,
        &ctx.accounts.instructions_sysvar,
        &crate::ID,
    )?;

    ctx.accounts.app_state.access_manager = new_access_manager;

    Ok(())
}

/// Revoke mint authority from IFT and transfer it to a new authority.
#[derive(Accounts)]
pub struct RevokeMintAuthority<'info> {
    /// IFT app state (will be closed)
    #[account(
        mut,
        close = payer,
        seeds = [IFT_APP_STATE_SEED, app_state.mint.as_ref()],
        bump = app_state.bump
    )]
    pub app_state: Account<'info, IFTAppState>,

    /// SPL Token mint - authority will be transferred
    #[account(
        mut,
        address = app_state.mint
    )]
    pub mint: Account<'info, Mint>,

    /// Current mint authority PDA (IFT's)
    /// CHECK: Derived PDA verified by seeds
    #[account(
        seeds = [MINT_AUTHORITY_SEED, mint.key().as_ref()],
        bump = app_state.mint_authority_bump
    )]
    pub mint_authority: AccountInfo<'info>,

    /// New mint authority to receive ownership
    /// CHECK: Can be any pubkey chosen by admin
    pub new_mint_authority: AccountInfo<'info>,

    /// CHECK: Validated via seeds constraint using stored `access_manager` program ID
    #[account(
        seeds = [access_manager::state::AccessManager::SEED],
        bump,
        seeds::program = app_state.access_manager
    )]
    pub access_manager: AccountInfo<'info>,

    /// Admin signer (must have ADMIN_ROLE)
    pub admin: Signer<'info>,

    /// Payer receives rent from closed app_state
    #[account(mut)]
    pub payer: Signer<'info>,

    /// Instructions sysvar for access manager verification
    /// CHECK: Address constraint verifies this is the instructions sysvar
    #[account(address = anchor_lang::solana_program::sysvar::instructions::ID)]
    pub instructions_sysvar: AccountInfo<'info>,

    pub token_program: Program<'info, Token>,
}

/// Revoke mint authority and close IFT app state.
/// Transfers mint authority back to the specified new authority.
pub fn revoke_mint_authority(ctx: Context<RevokeMintAuthority>) -> Result<()> {
    access_manager::require_role(
        &ctx.accounts.access_manager,
        solana_ibc_types::roles::ADMIN_ROLE,
        &ctx.accounts.admin,
        &ctx.accounts.instructions_sysvar,
        &crate::ID,
    )?;

    let mint_key = ctx.accounts.mint.key();
    let mint_authority_bump = ctx.accounts.app_state.mint_authority_bump;

    let seeds = &[
        MINT_AUTHORITY_SEED,
        mint_key.as_ref(),
        &[mint_authority_bump],
    ];
    let signer_seeds = &[&seeds[..]];

    anchor_spl::token::set_authority(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            anchor_spl::token::SetAuthority {
                current_authority: ctx.accounts.mint_authority.to_account_info(),
                account_or_mint: ctx.accounts.mint.to_account_info(),
            },
            signer_seeds,
        ),
        anchor_spl::token::spl_token::instruction::AuthorityType::MintTokens,
        Some(ctx.accounts.new_mint_authority.key()),
    )?;

    let clock = Clock::get()?;
    emit!(MintAuthorityRevoked {
        mint: ctx.accounts.mint.key(),
        new_authority: ctx.accounts.new_mint_authority.key(),
        timestamp: clock.unix_timestamp,
    });

    Ok(())
}

#[cfg(test)]
mod tests;
