use crate::errors::IbcRouterError;
use crate::state::*;
use crate::utils::ics24_host;
use anchor_lang::prelude::*;
use crate::instructions::recv_packet::NoopEvent;

#[derive(Accounts)]
#[instruction(msg: MsgTimeoutPacket)]
pub struct TimeoutPacket<'info> {
    #[account(
        seeds = [ROUTER_STATE_SEED],
        bump
    )]
    pub router_state: Account<'info, RouterState>,

    #[account(
        seeds = [PORT_REGISTRY_SEED, msg.packet.payloads[0].source_port.as_bytes()],
        bump
    )]
    pub port_registry: Account<'info, PortRegistry>,

    #[account(
        mut,
        seeds = [
            PACKET_COMMITMENT_SEED,
            msg.packet.source_client.as_bytes(),
            &msg.packet.sequence.to_le_bytes()
        ],
        bump,
        close = payer
    )]
    pub packet_commitment: Account<'info, Commitment>,

    pub relayer: Signer<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
    // TODO: Add light client accounts for proof verification
}

pub fn timeout_packet(ctx: Context<TimeoutPacket>, msg: MsgTimeoutPacket) -> Result<()> {
    let router_state = &ctx.accounts.router_state;
    let packet_commitment = &ctx.accounts.packet_commitment;

    require!(
        ctx.accounts.relayer.key() == router_state.authority,
        IbcRouterError::UnauthorizedSender
    );

    require!(
        msg.packet.payloads.len() == 1,
        IbcRouterError::MultiPayloadPacketNotSupported
    );

    // TODO: Verify counterparty client ID

    // TODO: Verify non-membership proof via CPI to light client
    // This would verify that the packet was NOT received on the counterparty
    // and that the counterparty's timestamp is past the timeout

    let expected_commitment = ics24_host::packet_commitment_bytes32(&msg.packet);
    if packet_commitment.value != expected_commitment {
        // No-op case - commitment doesn't exist or mismatch
        emit!(NoopEvent {});
        return Ok(());
    }

    // TODO: CPI to IBC app's onTimeoutPacket

    // The account will be closed automatically by Anchor due to close = payer

    emit!(TimeoutPacketEvent {
        client_id: msg.packet.source_client.clone(),
        sequence: msg.packet.sequence,
        packet_data: msg.packet.try_to_vec().unwrap(),
    });

    Ok(())
}

#[event]
pub struct TimeoutPacketEvent {
    pub client_id: String,
    pub sequence: u64,
    pub packet_data: Vec<u8>,
}

