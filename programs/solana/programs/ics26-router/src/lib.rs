use anchor_lang::prelude::*;

pub mod errors;
pub mod instructions;
pub mod state;
pub mod utils;

use instructions::*;
use state::{CounterpartyInfo, MsgAckPacket, MsgRecvPacket, MsgSendPacket, MsgTimeoutPacket};

declare_id!("HsCyuYgKgoN9wUPiJyNZvvWg2N1uyZhDjvJfKJFu3jvU");

#[cfg(test)]
pub fn get_router_program_path() -> &'static str {
    use std::sync::OnceLock;
    static PATH: OnceLock<String> = OnceLock::new();

    PATH.get_or_init(|| {
        std::env::var("ROUTER_PROGRAM_PATH")
            .unwrap_or_else(|_| "../../target/deploy/ics26_router".to_string())
    })
}

#[program]
pub mod ics26_router {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>, authority: Pubkey) -> Result<()> {
        instructions::initialize(ctx, authority)
    }

    pub fn add_ibc_app(ctx: Context<AddIbcApp>, port_id: String) -> Result<()> {
        instructions::add_ibc_app(ctx, port_id)
    }

    pub fn send_packet(ctx: Context<SendPacket>, msg: MsgSendPacket) -> Result<u64> {
        instructions::send_packet(ctx, msg)
    }

    pub fn recv_packet(ctx: Context<RecvPacket>, msg: MsgRecvPacket) -> Result<()> {
        instructions::recv_packet(ctx, msg)
    }

    pub fn ack_packet(ctx: Context<AckPacket>, msg: MsgAckPacket) -> Result<()> {
        instructions::ack_packet(ctx, msg)
    }

    pub fn timeout_packet(ctx: Context<TimeoutPacket>, msg: MsgTimeoutPacket) -> Result<()> {
        instructions::timeout_packet(ctx, msg)
    }

    pub fn store_commitment(
        ctx: Context<StoreCommitment>,
        path_hash: [u8; 32],
        commitment: [u8; 32],
    ) -> Result<()> {
        instructions::store_commitment(ctx, path_hash, commitment)
    }

    pub fn get_commitment(ctx: Context<GetCommitment>, path_hash: [u8; 32]) -> Result<[u8; 32]> {
        instructions::get_commitment(ctx, path_hash)
    }

    pub fn add_client(
        ctx: Context<AddClient>,
        client_id: String,
        counterparty_info: CounterpartyInfo,
    ) -> Result<()> {
        instructions::add_client(ctx, client_id, counterparty_info)
    }

    pub fn update_client(
        ctx: Context<UpdateClient>,
        client_id: String,
        active: bool,
    ) -> Result<()> {
        instructions::update_client(ctx, client_id, active)
    }
}
