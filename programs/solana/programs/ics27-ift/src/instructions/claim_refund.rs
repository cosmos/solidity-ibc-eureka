//! Claim refund instruction for IFT
//!
//! This instruction allows anyone to claim a refund for a pending transfer
//! after the GMP result has been recorded (either ack or timeout).

use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};
use solana_ibc_types::{CallResultStatus, GMPCallResult};

use crate::constants::{IFT_APP_STATE_SEED, MINT_AUTHORITY_SEED, PENDING_TRANSFER_SEED};
use crate::errors::IFTError;
use crate::events::{IFTTransferCompleted, IFTTransferRefunded, RefundReason};
use crate::evm_selectors::ERROR_ACK_COMMITMENT;
use crate::helpers::mint_to_account;
use crate::state::{IFTAppState, PendingTransfer};

/// Accounts for the `claim_refund` instruction
#[derive(Accounts)]
#[instruction(client_id: String, sequence: u64)]
pub struct ClaimRefund<'info> {
    /// IFT app state
    #[account(
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
            client_id.as_bytes(),
            &sequence.to_le_bytes()
        ],
        bump = pending_transfer.bump,
        constraint = pending_transfer.mint == app_state.mint @ IFTError::PendingTransferNotFound,
        constraint = pending_transfer.client_id == client_id @ IFTError::GmpResultClientMismatch,
        constraint = pending_transfer.sequence == sequence @ IFTError::GmpResultSequenceMismatch,
    )]
    pub pending_transfer: Account<'info, PendingTransfer>,

    /// GMP result account - proves the ack/timeout happened
    /// This is a cross-program account owned by the GMP program
    #[account(
        seeds = [GMPCallResult::SEED, client_id.as_bytes(), &sequence.to_le_bytes()],
        seeds::program = app_state.gmp_program,
        bump,
    )]
    pub gmp_result: Account<'info, ics27_gmp::state::GMPCallResultAccount>,

    /// SPL Token mint
    #[account(mut, address = app_state.mint)]
    pub mint: Account<'info, Mint>,

    /// Mint authority PDA
    /// CHECK: Derived PDA verified by seeds constraint
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

    /// Payer receives rent from closed `PendingTransfer` account
    #[account(mut)]
    pub payer: Signer<'info>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

/// Process refund claim based on GMP result
pub fn claim_refund(ctx: Context<ClaimRefund>, client_id: String, sequence: u64) -> Result<()> {
    let pending = &ctx.accounts.pending_transfer;
    let gmp_result = &ctx.accounts.gmp_result;
    let clock = Clock::get()?;

    // Verify the GMP result matches expectations
    require!(
        gmp_result.sender == crate::ID,
        IFTError::GmpResultSenderMismatch
    );
    require!(
        gmp_result.source_client == client_id,
        IFTError::GmpResultClientMismatch
    );
    require!(
        gmp_result.sequence == sequence,
        IFTError::GmpResultSequenceMismatch
    );

    match gmp_result.status {
        CallResultStatus::Timeout => {
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
                reason: RefundReason::Timeout,
                timestamp: clock.unix_timestamp,
            });
        }
        CallResultStatus::Acknowledgement(commitment) => {
            if commitment == ERROR_ACK_COMMITMENT {
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
            } else {
                emit!(IFTTransferCompleted {
                    mint: ctx.accounts.app_state.mint,
                    client_id: pending.client_id.clone(),
                    sequence: pending.sequence,
                    sender: pending.sender,
                    amount: pending.amount,
                    timestamp: clock.unix_timestamp,
                });
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ics26_router::utils::ics24::packet_acknowledgement_commitment_bytes32;

    #[test]
    fn test_error_ack_commitment_matches_runtime_computation() {
        // Verify the precomputed constant matches runtime computation
        let error_ack = ics26_router::utils::ics24::UNIVERSAL_ERROR_ACK;
        let computed =
            packet_acknowledgement_commitment_bytes32(std::slice::from_ref(&error_ack.to_vec()))
                .expect("single ack is never empty");

        assert_eq!(
            ERROR_ACK_COMMITMENT, computed,
            "Precomputed ERROR_ACK_COMMITMENT must match runtime computation"
        );
    }

    #[test]
    fn test_error_ack_commitment_is_valid() {
        // The commitment should be 32 bytes and not all zeros
        assert_eq!(ERROR_ACK_COMMITMENT.len(), 32);
        assert!(ERROR_ACK_COMMITMENT.iter().any(|&b| b != 0));
    }
}
