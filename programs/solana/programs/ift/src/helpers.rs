//! Shared helper functions for IFT operations

use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, MintTo, Token, TokenAccount};

use crate::constants::MINT_AUTHORITY_SEED;

/// Mint tokens to an account using the IFT mint authority PDA
pub fn mint_to_account<'info>(
    mint: &Account<'info, Mint>,
    to: &Account<'info, TokenAccount>,
    mint_authority: &AccountInfo<'info>,
    mint_authority_bump: u8,
    token_program: &Program<'info, Token>,
    amount: u64,
) -> Result<()> {
    let mint_key = mint.key();
    let seeds = &[
        MINT_AUTHORITY_SEED,
        mint_key.as_ref(),
        &[mint_authority_bump],
    ];
    let signer_seeds = &[&seeds[..]];

    let mint_accounts = MintTo {
        mint: mint.to_account_info(),
        to: to.to_account_info(),
        authority: mint_authority.to_account_info(),
    };
    let mint_ctx =
        CpiContext::new_with_signer(token_program.to_account_info(), mint_accounts, signer_seeds);
    token::mint_to(mint_ctx, amount)
}
