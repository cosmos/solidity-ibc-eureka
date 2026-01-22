use anchor_lang::prelude::*;

pub mod constants;
pub mod errors;
pub mod events;
pub mod instructions;
pub mod router_cpi;
pub mod state;
#[cfg(test)]
pub mod test_utils;
pub mod utils;

use instructions::client::MigrateClientParams;
use instructions::*;
use state::{
    CounterpartyInfo, MsgAckPacket, MsgCleanupChunks, MsgRecvPacket, MsgSendPacket,
    MsgTimeoutPacket, MsgUploadChunk,
};

declare_id!("FRGF7cthWUvDvAHMUARUHFycyUK2VDUtBchmkwrz7hgx");

#[program]
pub mod ics26_router {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>, access_manager: Pubkey) -> Result<()> {
        instructions::initialize(ctx, access_manager)
    }

    pub fn add_ibc_app(ctx: Context<AddIbcApp>, port_id: String) -> Result<()> {
        instructions::add_ibc_app(ctx, port_id)
    }

    pub fn send_packet(ctx: Context<SendPacket>, msg: MsgSendPacket) -> Result<u64> {
        instructions::send_packet(ctx, msg)
    }

    pub fn recv_packet<'info>(
        ctx: Context<'_, '_, '_, 'info, RecvPacket<'info>>,
        msg: MsgRecvPacket,
    ) -> Result<()> {
        instructions::recv_packet(ctx, msg)
    }

    pub fn ack_packet<'info>(
        ctx: Context<'_, '_, '_, 'info, AckPacket<'info>>,
        msg: MsgAckPacket,
    ) -> Result<()> {
        instructions::ack_packet(ctx, msg)
    }

    pub fn timeout_packet<'info>(
        ctx: Context<'_, '_, '_, 'info, TimeoutPacket<'info>>,
        msg: MsgTimeoutPacket,
    ) -> Result<()> {
        instructions::timeout_packet(ctx, msg)
    }

    pub fn add_client(
        ctx: Context<AddClient>,
        client_id: String,
        counterparty_info: CounterpartyInfo,
    ) -> Result<()> {
        instructions::add_client(ctx, client_id, counterparty_info)
    }

    pub fn migrate_client(
        ctx: Context<MigrateClient>,
        client_id: String,
        params: MigrateClientParams,
    ) -> Result<()> {
        instructions::migrate_client(ctx, client_id, params)
    }

    pub fn upload_payload_chunk(
        ctx: Context<UploadPayloadChunk>,
        msg: MsgUploadChunk,
    ) -> Result<()> {
        instructions::upload_payload_chunk(ctx, msg)
    }

    pub fn upload_proof_chunk(ctx: Context<UploadProofChunk>, msg: MsgUploadChunk) -> Result<()> {
        instructions::upload_proof_chunk(ctx, msg)
    }

    pub fn cleanup_chunks<'info>(
        ctx: Context<'_, '_, '_, 'info, CleanupChunks<'info>>,
        msg: MsgCleanupChunks,
    ) -> Result<()> {
        instructions::cleanup_chunks(ctx, msg)
    }

    pub fn set_access_manager(
        ctx: Context<SetAccessManager>,
        new_access_manager: Pubkey,
    ) -> Result<()> {
        instructions::set_access_manager(ctx, new_access_manager)
    }
}
