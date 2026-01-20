use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};

use crate::constants::*;
use crate::errors::IFTError;
use crate::events::{IFTTransferCompleted, IFTTransferRefunded, RefundReason};
use crate::helpers::mint_to_account;
use crate::state::{IFTAppState, PendingTransfer};

#[derive(Accounts)]
#[instruction(msg: solana_ibc_types::OnAcknowledgementPacketMsg)]
pub struct OnAckPacket<'info> {
    #[account(
        mut,
        seeds = [IFT_APP_STATE_SEED, app_state.mint.as_ref()],
        bump = app_state.bump
    )]
    pub app_state: Account<'info, IFTAppState>,

    /// Pending transfer to process
    #[account(
        mut,
        close = payer,
        seeds = [
            PENDING_TRANSFER_SEED,
            app_state.mint.as_ref(),
            pending_transfer.client_id.as_bytes(),
            &pending_transfer.sequence.to_le_bytes()
        ],
        bump = pending_transfer.bump,
        constraint = pending_transfer.mint == app_state.mint @ IFTError::PendingTransferNotFound
    )]
    pub pending_transfer: Account<'info, PendingTransfer>,

    /// SPL Token mint (for refunds on failure)
    #[account(
        mut,
        address = app_state.mint
    )]
    pub mint: Account<'info, Mint>,

    /// Mint authority PDA (for refunds)
    /// CHECK: Derived PDA
    #[account(
        seeds = [MINT_AUTHORITY_SEED, mint.key().as_ref()],
        bump = app_state.mint_authority_bump
    )]
    pub mint_authority: AccountInfo<'info>,

    /// Original sender's token account (for refunds)
    #[account(
        mut,
        constraint = sender_token_account.mint == mint.key() @ IFTError::TokenAccountOwnerMismatch,
        constraint = sender_token_account.owner == pending_transfer.sender @ IFTError::TokenAccountOwnerMismatch
    )]
    pub sender_token_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

pub fn on_acknowledgement_packet(
    ctx: Context<OnAckPacket>,
    msg: solana_ibc_types::OnAcknowledgementPacketMsg,
) -> Result<()> {
    let pending = &ctx.accounts.pending_transfer;
    let clock = Clock::get()?;

    let is_success = parse_gmp_acknowledgement(&msg.acknowledgement);

    if is_success {
        emit!(IFTTransferCompleted {
            mint: ctx.accounts.app_state.mint,
            client_id: pending.client_id.clone(),
            sequence: pending.sequence,
            sender: pending.sender,
            amount: pending.amount,
            timestamp: clock.unix_timestamp,
        });
    } else {
        mint_to_account(
            &ctx.accounts.mint,
            &ctx.accounts.sender_token_account,
            &ctx.accounts.mint_authority,
            ctx.accounts.app_state.mint_authority_bump,
            &ctx.accounts.token_program,
            pending.amount,
        )?;

        emit!(IFTTransferRefunded {
            mint: ctx.accounts.app_state.mint,
            client_id: pending.client_id.clone(),
            sequence: pending.sequence,
            sender: pending.sender,
            amount: pending.amount,
            reason: RefundReason::Failed,
            timestamp: clock.unix_timestamp,
        });
    }

    Ok(())
}

/// Returns true if ack is not the universal error acknowledgement
fn parse_gmp_acknowledgement(ack: &[u8]) -> bool {
    ack != ics26_router::utils::ics24::UNIVERSAL_ERROR_ACK
}

#[cfg(test)]
mod tests;
