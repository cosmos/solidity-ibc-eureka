use anchor_lang::prelude::*;

pub mod constants;
pub mod errors;
pub mod instructions;
pub mod router_cpi;
pub mod state;
#[cfg(test)]
pub mod test_utils;
pub mod utils;

use instructions::*;
use state::{
    CounterpartyInfo, MsgAckPacket, MsgCleanupChunks, MsgRecvPacket, MsgSendPacket,
    MsgTimeoutPacket, MsgUploadChunk,
};

declare_id!("FRGF7cthWUvDvAHMUARUHFycyUK2VDUtBchmkwrz7hgx");

#[cfg(test)]
pub fn get_router_program_path() -> &'static str {
    use std::sync::OnceLock;
    static PATH: OnceLock<String> = OnceLock::new();

    PATH.get_or_init(|| {
        std::env::var("ROUTER_PROGRAM_PATH")
            .unwrap_or_else(|_| "../../target/deploy/ics26_router".to_string())
    })
}

#[cfg(test)]
pub fn get_mock_client_program_path() -> &'static str {
    use std::sync::OnceLock;
    static PATH: OnceLock<String> = OnceLock::new();

    PATH.get_or_init(|| {
        std::env::var("MOCK_CLIENT_PROGRAM_PATH")
            .unwrap_or_else(|_| "../../target/deploy/mock_light_client".to_string())
    })
}

#[cfg(test)]
pub fn get_mock_ibc_app_program_path() -> &'static str {
    use std::sync::OnceLock;
    static PATH: OnceLock<String> = OnceLock::new();

    PATH.get_or_init(|| {
        std::env::var("MOCK_IBC_APP_PROGRAM_PATH")
            .unwrap_or_else(|_| "../../target/deploy/mock_ibc_app".to_string())
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

    pub fn upload_payload_chunk(
        ctx: Context<UploadPayloadChunk>,
        msg: MsgUploadChunk,
    ) -> Result<()> {
        instructions::upload_payload_chunk(ctx, msg)
    }

    pub fn upload_proof_chunk(
        ctx: Context<UploadProofChunk>,
        msg: MsgUploadChunk,
    ) -> Result<()> {
        instructions::upload_proof_chunk(ctx, msg)
    }

    pub fn cleanup_chunks<'info>(ctx: Context<'_, '_, '_, 'info, CleanupChunks<'info>>, msg: MsgCleanupChunks) -> Result<()> {
        instructions::cleanup_chunks(ctx, msg)
    }
}
