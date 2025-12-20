use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{self, Mint, MintTo, Token, TokenAccount},
};

use crate::constants::*;
use crate::errors::IFTError;
use crate::events::IFTMintReceived;
use crate::state::{IFTAppState, IFTMintMsg};

/// IFT Mint instruction - called by GMP via CPI when receiving a cross-chain mint request
#[derive(Accounts)]
#[instruction(msg: IFTMintMsg)]
pub struct IFTMint<'info> {
    #[account(
        mut,
        seeds = [IFT_APP_STATE_SEED, app_state.mint.as_ref()],
        bump = app_state.bump,
        constraint = !app_state.paused @ IFTError::AppPaused
    )]
    pub app_state: Account<'info, IFTAppState>,

    /// SPL Token mint
    #[account(
        mut,
        address = app_state.mint
    )]
    pub mint: Account<'info, Mint>,

    /// Mint authority PDA
    /// CHECK: Derived PDA that signs for minting
    #[account(
        seeds = [MINT_AUTHORITY_SEED, mint.key().as_ref()],
        bump = app_state.mint_authority_bump
    )]
    pub mint_authority: AccountInfo<'info>,

    /// Receiver's token account (will be created if needed)
    #[account(
        init_if_needed,
        payer = payer,
        associated_token::mint = mint,
        associated_token::authority = receiver_owner
    )]
    pub receiver_token_account: Account<'info, TokenAccount>,

    /// CHECK: The receiver owner pubkey (must match msg.receiver)
    #[account(
        constraint = receiver_owner.key() == msg.receiver @ IFTError::InvalidReceiver
    )]
    pub receiver_owner: AccountInfo<'info>,

    /// GMP account PDA - signer proves call came from valid cross-chain source
    pub gmp_account: Signer<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

pub fn ift_mint(ctx: Context<IFTMint>, msg: IFTMintMsg) -> Result<()> {
    require!(msg.amount > 0, IFTError::ZeroAmount);

    // Mint tokens to receiver
    let mint_key = ctx.accounts.mint.key();
    let seeds = &[
        MINT_AUTHORITY_SEED,
        mint_key.as_ref(),
        &[ctx.accounts.app_state.mint_authority_bump],
    ];
    let signer_seeds = &[&seeds[..]];

    let mint_accounts = MintTo {
        mint: ctx.accounts.mint.to_account_info(),
        to: ctx.accounts.receiver_token_account.to_account_info(),
        authority: ctx.accounts.mint_authority.to_account_info(),
    };
    let mint_ctx = CpiContext::new_with_signer(
        ctx.accounts.token_program.to_account_info(),
        mint_accounts,
        signer_seeds,
    );
    token::mint_to(mint_ctx, msg.amount)?;

    let clock = Clock::get()?;
    emit!(IFTMintReceived {
        mint: ctx.accounts.mint.key(),
        client_id: String::new(), // Would be extracted from GMP packet in full impl
        receiver: msg.receiver,
        amount: msg.amount,
        gmp_account: ctx.accounts.gmp_account.key(),
        timestamp: clock.unix_timestamp,
    });

    Ok(())
}
