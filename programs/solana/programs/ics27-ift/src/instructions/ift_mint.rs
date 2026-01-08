use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{Mint, Token, TokenAccount},
};

use crate::constants::*;
use crate::errors::IFTError;
use crate::events::IFTMintReceived;
use crate::helpers::mint_to_account;
use crate::state::{IFTAppState, IFTBridge, IFTMintMsg};

/// IFT Mint instruction - called by GMP via CPI when receiving a cross-chain mint request
#[derive(Accounts)]
#[instruction(msg: IFTMintMsg)]
pub struct IFTMint<'info> {
    #[account(
        mut,
        seeds = [IFT_APP_STATE_SEED, app_state.mint.as_ref()],
        bump = app_state.bump,
    )]
    pub app_state: Account<'info, IFTAppState>,

    /// IFT bridge - provides counterparty info for GMP account validation
    #[account(
        seeds = [IFT_BRIDGE_SEED, app_state.mint.as_ref(), msg.client_id.as_bytes()],
        bump = ift_bridge.bump,
        constraint = ift_bridge.active @ IFTError::BridgeNotActive
    )]
    pub ift_bridge: Account<'info, IFTBridge>,

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

    /// CHECK: GMP program for PDA derivation
    #[account(address = app_state.gmp_program @ IFTError::InvalidGmpProgram)]
    pub gmp_program: AccountInfo<'info>,

    /// GMP account PDA - validated to match counterparty bridge
    pub gmp_account: Signer<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

pub fn ift_mint(ctx: Context<IFTMint>, msg: IFTMintMsg) -> Result<()> {
    require!(msg.amount > 0, IFTError::ZeroAmount);
    validate_gmp_account(
        &ctx.accounts.gmp_account.key(),
        &ctx.accounts.ift_bridge.client_id,
        &ctx.accounts.ift_bridge.counterparty_ift_address,
        &ctx.accounts.gmp_program.key(),
        msg.gmp_account_bump,
    )?;

    mint_to_account(
        &ctx.accounts.mint,
        &ctx.accounts.receiver_token_account,
        &ctx.accounts.mint_authority,
        ctx.accounts.app_state.mint_authority_bump,
        &ctx.accounts.token_program,
        msg.amount,
    )?;

    let clock = Clock::get()?;
    emit!(IFTMintReceived {
        mint: ctx.accounts.mint.key(),
        client_id: ctx.accounts.ift_bridge.client_id.clone(),
        receiver: msg.receiver,
        amount: msg.amount,
        gmp_account: ctx.accounts.gmp_account.key(),
        timestamp: clock.unix_timestamp,
    });

    Ok(())
}

/// Validate GMP account is derived from expected counterparty bridge
///
/// For IFT, the sender is the counterparty IFT address and salt must be empty.
fn validate_gmp_account(
    gmp_account: &Pubkey,
    client_id: &str,
    counterparty_address: &str,
    gmp_program: &Pubkey,
    bump: u8,
) -> Result<()> {
    let sender_hash = solana_sha256_hasher::hash(counterparty_address.as_bytes()).to_bytes();
    let expected_pda = Pubkey::create_program_address(
        &[
            b"gmp_account",
            client_id.as_bytes(),
            &sender_hash,
            &[], // Salt
            &[bump],
        ],
        gmp_program,
    )
    .map_err(|_| IFTError::InvalidGmpAccount)?;
    require!(*gmp_account == expected_pda, IFTError::InvalidGmpAccount);
    Ok(())
}

#[cfg(test)]
mod tests;
