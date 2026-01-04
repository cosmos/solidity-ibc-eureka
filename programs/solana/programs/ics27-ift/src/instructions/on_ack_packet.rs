use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, MintTo, Token, TokenAccount};

use crate::constants::*;
use crate::errors::IFTError;
use crate::events::{IFTTransferCompleted, IFTTransferRefunded, RefundReason};
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

    /// Router program calling this instruction
    pub router_program: Program<'info, ics26_router::program::Ics26Router>,

    /// CHECK: Instructions sysvar for CPI validation
    #[account(address = anchor_lang::solana_program::sysvar::instructions::ID)]
    pub instruction_sysvar: AccountInfo<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

pub fn on_acknowledgement_packet(
    ctx: Context<OnAckPacket>,
    msg: solana_ibc_types::OnAcknowledgementPacketMsg,
) -> Result<()> {
    // Verify this function is called via CPI from the authorized router
    solana_ibc_types::validate_cpi_caller(
        &ctx.accounts.instruction_sysvar,
        &ctx.accounts.router_program.key(),
        &crate::ID,
    )
    .map_err(IFTError::from)?;

    let pending = &ctx.accounts.pending_transfer;
    let clock = Clock::get()?;

    // Parse acknowledgement to determine success/failure
    let is_success = parse_gmp_acknowledgement(&msg.acknowledgement);

    if is_success {
        // Transfer completed successfully - just clear pending transfer
        emit!(IFTTransferCompleted {
            mint: ctx.accounts.app_state.mint,
            client_id: pending.client_id.clone(),
            sequence: pending.sequence,
            sender: pending.sender,
            amount: pending.amount,
            timestamp: clock.unix_timestamp,
        });
    } else {
        // Transfer failed - refund tokens to sender
        let mint_key = ctx.accounts.mint.key();
        let seeds = &[
            MINT_AUTHORITY_SEED,
            mint_key.as_ref(),
            &[ctx.accounts.app_state.mint_authority_bump],
        ];
        let signer_seeds = &[&seeds[..]];

        let mint_accounts = MintTo {
            mint: ctx.accounts.mint.to_account_info(),
            to: ctx.accounts.sender_token_account.to_account_info(),
            authority: ctx.accounts.mint_authority.to_account_info(),
        };
        let mint_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            mint_accounts,
            signer_seeds,
        );
        token::mint_to(mint_ctx, pending.amount)?;

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

    // Pending transfer account is closed via Anchor's close constraint
    Ok(())
}

/// Parse GMP acknowledgement to determine success/failure
///
/// IBC acknowledgements follow the ICS-04 pattern:
/// - Error: Raw 32-byte `UNIVERSAL_ERROR_ACK` (`sha256("UNIVERSAL_ERROR_ACKNOWLEDGEMENT")`)
/// - Success: Protobuf-encoded `GmpAcknowledgement { result: bytes }`
///
/// We only need to check if the ack equals the error constant to determine failure.
fn parse_gmp_acknowledgement(ack: &[u8]) -> bool {
    ack != ics26_router::utils::ics24::UNIVERSAL_ERROR_ACK
}

#[cfg(test)]
mod tests;
