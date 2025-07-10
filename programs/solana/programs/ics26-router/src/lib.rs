#![allow(unexpected_cfgs)]
#![allow(deprecated)]

use anchor_lang::prelude::*;
use anchor_lang::solana_program::keccak::hash as keccak256;

pub mod errors;
pub mod instructions;
pub mod state;
pub mod utils;

use instructions::*;

declare_id!("HsCyuYgKgoN9wUPiJyNZvvWg2N1uyZhDjvJfKJFu3jvU");

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
}

