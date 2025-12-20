use anchor_lang::prelude::*;

pub mod constants;
pub mod errors;
pub mod events;
pub mod evm_selectors;
pub mod gmp_cpi;
pub mod instructions;
pub mod state;

#[cfg(test)]
pub mod test_utils;

use instructions::*;
use state::{IFTMintMsg, IFTTransferMsg, RegisterIFTBridgeMsg};

declare_id!("Bedm6bv1H5oCzBYZdQtesZTgHeUjLp7XuUmnhiYwXHn5");

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

    /// Handle acknowledgement packet (called by router via CPI)
    pub fn on_acknowledgement_packet(
        ctx: Context<OnAckPacket>,
        msg: solana_ibc_types::OnAcknowledgementPacketMsg,
    ) -> Result<()> {
        instructions::on_acknowledgement_packet(ctx, msg)
    }

    /// Handle timeout packet (called by router via CPI)
    pub fn on_timeout_packet(
        ctx: Context<OnTimeoutPacket>,
        msg: solana_ibc_types::OnTimeoutPacketMsg,
    ) -> Result<()> {
        instructions::on_timeout_packet(ctx, msg)
    }

    /// Pause the IFT app (admin only)
    pub fn pause_app(ctx: Context<PauseApp>) -> Result<()> {
        instructions::pause_app(ctx)
    }

    /// Unpause the IFT app (admin only)
    pub fn unpause_app(ctx: Context<UnpauseApp>) -> Result<()> {
        instructions::unpause_app(ctx)
    }

    /// Set the access manager program (admin only)
    pub fn set_access_manager(
        ctx: Context<SetAccessManager>,
        new_access_manager: Pubkey,
    ) -> Result<()> {
        instructions::set_access_manager(ctx, new_access_manager)
    }
}
