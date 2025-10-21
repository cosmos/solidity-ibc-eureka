use anchor_lang::prelude::*;
use solana_ibc_macros::ibc_app;

pub mod constants;
pub mod errors;
pub mod events;
pub mod instructions;
pub mod proto;
pub mod router_cpi;
pub mod state;
pub mod utils;

#[cfg(test)]
pub mod test_utils;

use instructions::*;
use state::SendCallMsg;

declare_id!("3W3h4WSE8J9vFzVN8TGFGc9Uchbry3M4MBz4icdSWcFi");

#[cfg(test)]
pub fn get_gmp_program_path() -> &'static str {
    use std::sync::OnceLock;
    static PATH: OnceLock<String> = OnceLock::new();

    PATH.get_or_init(|| {
        std::env::var("GMP_PROGRAM_PATH")
            .unwrap_or_else(|_| "../../target/deploy/ics27_gmp".to_string())
    })
}

#[ibc_app]
pub mod ics27_gmp {
    use super::*;

    /// Initialize the ICS27 GMP application
    pub fn initialize(ctx: Context<Initialize>, router_program: Pubkey) -> Result<()> {
        instructions::initialize(ctx, router_program)
    }

    /// Send a GMP call packet
    pub fn send_call(ctx: Context<SendCall>, msg: SendCallMsg) -> Result<u64> {
        instructions::send_call(ctx, msg)
    }

    /// IBC packet receive handler (called by router via CPI)
    pub fn on_recv_packet<'info>(
        ctx: Context<'_, '_, '_, 'info, OnRecvPacket<'info>>,
        msg: solana_ibc_types::OnRecvPacketMsg,
    ) -> Result<Vec<u8>> {
        instructions::on_recv_packet(ctx, msg)
    }

    /// IBC acknowledgement handler (called by router via CPI)
    pub fn on_acknowledgement_packet(
        ctx: Context<OnAckPacket>,
        msg: solana_ibc_types::OnAcknowledgementPacketMsg,
    ) -> Result<()> {
        instructions::on_acknowledgement_packet(ctx, msg)
    }

    /// IBC timeout handler (called by router via CPI)
    pub fn on_timeout_packet(
        ctx: Context<OnTimeoutPacket>,
        msg: solana_ibc_types::OnTimeoutPacketMsg,
    ) -> Result<()> {
        instructions::on_timeout_packet(ctx, msg)
    }

    /// Pause the entire GMP app (admin only)
    pub fn pause_app(ctx: Context<PauseApp>) -> Result<()> {
        instructions::pause_app(ctx)
    }

    /// Unpause the entire GMP app (admin only)
    pub fn unpause_app(ctx: Context<UnpauseApp>) -> Result<()> {
        instructions::unpause_app(ctx)
    }

    /// Update app authority (admin only)
    pub fn update_authority(ctx: Context<UpdateAuthority>) -> Result<()> {
        instructions::update_authority(ctx)
    }
}
