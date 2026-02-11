//! Store and close `SolanaPayloadHint` accounts.
//!
//! The relayer stores a protobuf-encoded `GmpSolanaPayload` in a hint account
//! before calling `recv_packet` for ABI-encoded packets.

use anchor_lang::prelude::*;

use crate::errors::GMPError;
use crate::state::SolanaPayloadHint;

#[derive(Accounts)]
pub struct StorePayloadHint<'info> {
    #[account(
        init_if_needed,
        payer = payer,
        space = 8 + SolanaPayloadHint::INIT_SPACE,
        seeds = [SolanaPayloadHint::SEED, payer.key().as_ref()],
        bump,
    )]
    pub hint: Account<'info, SolanaPayloadHint>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn store_payload_hint(ctx: Context<StorePayloadHint>, data: Vec<u8>) -> Result<()> {
    require!(
        data.len() <= SolanaPayloadHint::MAX_DATA_LEN,
        GMPError::InvalidSolanaPayload
    );
    let hint = &mut ctx.accounts.hint;
    hint.bump = ctx.bumps.hint;
    hint.data = data;
    Ok(())
}

#[derive(Accounts)]
pub struct ClosePayloadHint<'info> {
    #[account(
        mut,
        close = payer,
        seeds = [SolanaPayloadHint::SEED, payer.key().as_ref()],
        bump = hint.bump,
    )]
    pub hint: Account<'info, SolanaPayloadHint>,

    #[account(mut)]
    pub payer: Signer<'info>,
}

pub fn close_payload_hint(_ctx: Context<ClosePayloadHint>) -> Result<()> {
    Ok(())
}
