use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token};

use crate::constants::*;
use crate::errors::IFTError;
use crate::events::IFTAppInitialized;
use crate::state::{AccountVersion, IFTAppState};

#[derive(Accounts)]
#[instruction(decimals: u8)]
pub struct Initialize<'info> {
    /// IFT app state PDA (to be created)
    #[account(
        init,
        payer = payer,
        space = 8 + IFTAppState::INIT_SPACE,
        seeds = [IFT_APP_STATE_SEED, mint.key().as_ref()],
        bump
    )]
    pub app_state: Account<'info, IFTAppState>,

    /// SPL Token mint (must already exist, IFT will take mint authority)
    #[account(mut)]
    pub mint: Account<'info, Mint>,

    /// Mint authority PDA - will become the mint authority
    /// CHECK: Derived PDA that will be set as mint authority
    #[account(
        seeds = [MINT_AUTHORITY_SEED, mint.key().as_ref()],
        bump
    )]
    pub mint_authority: AccountInfo<'info>,

    /// Current mint authority (must sign to transfer authority)
    pub current_mint_authority: Signer<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

pub fn initialize(
    ctx: Context<Initialize>,
    decimals: u8,
    access_manager: Pubkey,
    gmp_program: Pubkey,
) -> Result<()> {
    let mint = &ctx.accounts.mint;

    // Verify decimals match the mint
    require!(mint.decimals == decimals, IFTError::InvalidMintAuthority);

    // Transfer mint authority to IFT PDA
    let cpi_accounts = anchor_spl::token::SetAuthority {
        account_or_mint: ctx.accounts.mint.to_account_info(),
        current_authority: ctx.accounts.current_mint_authority.to_account_info(),
    };
    let cpi_program = ctx.accounts.token_program.to_account_info();
    let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);

    anchor_spl::token::set_authority(
        cpi_ctx,
        anchor_spl::token::spl_token::instruction::AuthorityType::MintTokens,
        Some(ctx.accounts.mint_authority.key()),
    )?;

    // Initialize app state
    let app_state = &mut ctx.accounts.app_state;
    app_state.version = AccountVersion::V1;
    app_state.paused = false;
    app_state.bump = ctx.bumps.app_state;
    app_state.mint = ctx.accounts.mint.key();
    app_state.mint_authority_bump = ctx.bumps.mint_authority;
    app_state.access_manager = access_manager;
    app_state.gmp_program = gmp_program;

    let clock = Clock::get()?;
    emit!(IFTAppInitialized {
        mint: ctx.accounts.mint.key(),
        decimals,
        access_manager,
        gmp_program,
        timestamp: clock.unix_timestamp,
    });

    Ok(())
}

#[cfg(test)]
mod tests;
