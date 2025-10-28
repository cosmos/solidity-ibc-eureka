use anchor_lang::prelude::*;
use solana_ibc_macros::ibc_app;
use solana_ibc_types::{OnAcknowledgementPacketMsg, OnRecvPacketMsg, OnTimeoutPacketMsg};

declare_id!("BQdrL2ngS1T7HhTgNuzR7gg81VtJQddhKv1vyJrvnF5z");

/// The ICS26 Router program ID that is authorized to call this instruction
pub const ICS26_ROUTER_ID: Pubkey = pubkey!("GbVfnimoJNUhg8S9tGKAYJx7SYsuRcDcwQSt4zCizqis");

pub mod errors;
pub mod instructions;
pub mod state;

use instructions::*;
pub use state::{PACKETS_ACKNOWLEDGED_OFFSET, PACKETS_RECEIVED_OFFSET, PACKETS_TIMED_OUT_OFFSET};

/// Dummy IBC Application Program
///
/// This program demonstrates how to implement the Solana IBC App interface.
///
/// It provides:
///
/// - `initialize`: Initialize the dummy IBC app
/// - `send_transfer`: Send a transfer packet via IBC (for testing)
/// - `send_packet`: Send an arbitrary packet via IBC (for flexible testing)
/// - `on_recv_packet`: Handles incoming packets and returns acknowledgements
/// - `on_acknowledgement_packet`: Handles packet acknowledgements
/// - `on_timeout_packet`: Handles packet timeouts
///
#[ibc_app]
pub mod dummy_ibc_app {
    use super::*;

    /// Initialize the dummy IBC app
    pub fn initialize(ctx: Context<Initialize>, authority: Pubkey) -> Result<()> {
        instructions::initialize(ctx, authority)
    }

    /// Send a transfer packet via IBC
    pub fn send_transfer(ctx: Context<SendTransfer>, msg: SendTransferMsg) -> Result<()> {
        instructions::send_transfer(ctx, msg)
    }

    /// Send an arbitrary packet via IBC
    pub fn send_packet(ctx: Context<SendPacket>, msg: SendPacketMsg) -> Result<()> {
        instructions::send_packet(ctx, msg)
    }

    /// Handle incoming packet
    /// Returns acknowledgement data via `set_return_data`
    pub fn on_recv_packet(ctx: Context<OnRecvPacket>, msg: OnRecvPacketMsg) -> Result<()> {
        instructions::on_recv_packet(ctx, msg)
    }

    /// Handle packet acknowledgement
    pub fn on_acknowledgement_packet(
        ctx: Context<OnAcknowledgementPacket>,
        msg: OnAcknowledgementPacketMsg,
    ) -> Result<()> {
        instructions::on_acknowledgement_packet(ctx, msg)
    }

    /// Handle packet timeout
    pub fn on_timeout_packet(ctx: Context<OnTimeoutPacket>, msg: OnTimeoutPacketMsg) -> Result<()> {
        instructions::on_timeout_packet(ctx, msg)
    }
}
