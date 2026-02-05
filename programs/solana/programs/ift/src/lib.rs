use anchor_lang::prelude::*;

pub mod constants;
pub mod errors;
pub mod events;
pub mod evm_selectors;
pub mod gmp_cpi;
pub mod helpers;
pub mod instructions;
pub mod state;

#[cfg(test)]
pub mod test_utils;

use instructions::*;
use state::{IFTMintMsg, IFTTransferMsg, RegisterIFTBridgeMsg, SetMintRateLimitMsg, SetPausedMsg};

declare_id!("DQU7WYvJTdpbLSzpLjHtCRF7wiaWe7thXwboafEN4kcy");

#[program]
pub mod ift {
    use super::*;

    /// Create a new SPL token mint for IFT
    pub fn create_spl_token(
        ctx: Context<CreateSplToken>,
        decimals: u8,
        admin: Pubkey,
        gmp_program: Pubkey,
    ) -> Result<()> {
        instructions::create_spl_token(ctx, decimals, admin, gmp_program)
    }

    /// Initialize IFT for an existing SPL token by transferring mint authority
    pub fn initialize_existing_token(
        ctx: Context<InitializeExistingToken>,
        admin: Pubkey,
        gmp_program: Pubkey,
    ) -> Result<()> {
        instructions::initialize_existing_token(ctx, admin, gmp_program)
    }

    /// Register an IFT bridge to a counterparty chain
    pub fn register_ift_bridge(
        ctx: Context<RegisterIFTBridge>,
        msg: RegisterIFTBridgeMsg,
    ) -> Result<()> {
        instructions::register_ift_bridge(ctx, msg)
    }

    /// Remove an IFT bridge
    pub fn remove_ift_bridge(ctx: Context<RemoveIFTBridge>, client_id: String) -> Result<()> {
        instructions::remove_ift_bridge(ctx, client_id)
    }

    /// Initiate an IFT transfer to another chain
    /// Burns tokens and sends GMP call to mint on destination
    pub fn ift_transfer(ctx: Context<IFTTransfer>, msg: IFTTransferMsg) -> Result<u64> {
        instructions::ift_transfer(ctx, msg)
    }

    /// Mint IFT tokens (called by GMP when receiving cross-chain transfer).
    pub fn ift_mint(ctx: Context<IFTMint>, msg: IFTMintMsg) -> Result<()> {
        instructions::ift_mint(ctx, msg)
    }

    /// Claim refund for a pending transfer after GMP result is recorded and proved ack/timeout.
    pub fn claim_refund(ctx: Context<ClaimRefund>, client_id: String, sequence: u64) -> Result<()> {
        instructions::claim_refund(ctx, client_id, sequence)
    }

    /// Set the admin authority (admin only)
    pub fn set_admin(ctx: Context<SetAdmin>, new_admin: Pubkey) -> Result<()> {
        instructions::set_admin(ctx, new_admin)
    }

    /// Revoke mint authority from IFT and transfer it to a new authority.
    pub fn revoke_mint_authority(ctx: Context<RevokeMintAuthority>) -> Result<()> {
        instructions::revoke_mint_authority(ctx)
    }

    /// Set the daily mint rate limit (admin only)
    pub fn set_mint_rate_limit(
        ctx: Context<SetMintRateLimit>,
        msg: SetMintRateLimitMsg,
    ) -> Result<()> {
        instructions::set_mint_rate_limit(ctx, msg)
    }

    /// Pause or unpause an IFT token (admin only)
    pub fn set_paused(ctx: Context<SetPaused>, msg: SetPausedMsg) -> Result<()> {
        instructions::set_paused(ctx, msg)
    }
}
