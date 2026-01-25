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
use state::{IFTMintMsg, IFTTransferMsg, RegisterIFTBridgeMsg};

declare_id!("DQU7WYvJTdpbLSzpLjHtCRF7wiaWe7thXwboafEN4kcy");

#[program]
pub mod ics27_ift {
    use super::*;

    /// Initialize the IFT application with a new SPL token mint
    pub fn initialize(
        ctx: Context<Initialize>,
        decimals: u8,
        access_manager: Pubkey,
        gmp_program: Pubkey,
    ) -> Result<()> {
        instructions::initialize(ctx, decimals, access_manager, gmp_program)
    }

    /// Register an IFT bridge to a counterparty chain
    pub fn register_ift_bridge(
        ctx: Context<RegisterIFTBridge>,
        msg: RegisterIFTBridgeMsg,
    ) -> Result<()> {
        instructions::register_ift_bridge(ctx, msg)
    }

    /// Remove an IFT bridge
    pub fn remove_ift_bridge(ctx: Context<RemoveIFTBridge>) -> Result<()> {
        instructions::remove_ift_bridge(ctx)
    }

    /// Initiate an IFT transfer to another chain
    /// Burns tokens and sends GMP call to mint on destination
    pub fn ift_transfer(ctx: Context<IFTTransfer>, msg: IFTTransferMsg) -> Result<u64> {
        instructions::ift_transfer(ctx, msg)
    }

    /// Mint IFT tokens (called by GMP when receiving cross-chain transfer)
    pub fn ift_mint(ctx: Context<IFTMint>, msg: IFTMintMsg) -> Result<()> {
        instructions::ift_mint(ctx, msg)
    }

    /// Claim refund for a pending transfer after GMP result is recorded and proved ack/timeout.
    pub fn claim_refund(ctx: Context<ClaimRefund>, client_id: String, sequence: u64) -> Result<()> {
        instructions::claim_refund(ctx, client_id, sequence)
    }

    /// Set the access manager program (admin only)
    pub fn set_access_manager(
        ctx: Context<SetAccessManager>,
        new_access_manager: Pubkey,
    ) -> Result<()> {
        instructions::set_access_manager(ctx, new_access_manager)
    }

    /// Revoke mint authority from IFT and transfer it to a new authority.
    pub fn revoke_mint_authority(ctx: Context<RevokeMintAuthority>) -> Result<()> {
        instructions::revoke_mint_authority(ctx)
    }
}
