use crate::errors::RouterError;
use crate::state::*;
use crate::utils::ics24;
use anchor_lang::prelude::*;

#[derive(Accounts)]
#[instruction(msg: MsgSendPacket)]
pub struct SendPacket<'info> {
    #[account(
        seeds = [ROUTER_STATE_SEED],
        bump
    )]
    pub router_state: Account<'info, RouterState>,

    #[account(
        seeds = [PORT_REGISTRY_SEED, msg.payload.source_port.as_bytes()],
        bump
    )]
    pub port_registry: Account<'info, PortRegistry>,

    #[account(
        mut,
        seeds = [CLIENT_SEQUENCE_SEED, msg.source_client.as_bytes()],
        bump
    )]
    pub client_sequence: Account<'info, ClientSequence>,

    #[account(
        init,
        payer = payer,
        space = 8 + Commitment::INIT_SPACE, // discriminator + commitment
        seeds = [
            PACKET_COMMITMENT_SEED,
            msg.source_client.as_bytes(),
            &client_sequence.next_sequence_send.to_le_bytes()
        ],
        bump
    )]
    pub packet_commitment: Account<'info, Commitment>,

    /// The IBC app calling this instruction
    pub app_caller: Signer<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,

    pub clock: Sysvar<'info, Clock>,

    #[account(
        seeds = [CLIENT_SEED, msg.source_client.as_bytes()],
        bump,
        constraint = client.active @ RouterError::ClientNotActive,
    )]
    pub client: Account<'info, Client>,
}

pub fn send_packet(ctx: Context<SendPacket>, msg: MsgSendPacket) -> Result<u64> {
    // TODO: Support multi-payload packets #602
    let port_registry = &ctx.accounts.port_registry;
    let client_sequence = &mut ctx.accounts.client_sequence;
    let packet_commitment = &mut ctx.accounts.packet_commitment;
    let clock = &ctx.accounts.clock;

    require!(
        ctx.accounts.app_caller.key() == port_registry.app_program_id,
        RouterError::UnauthorizedSender
    );

    let current_timestamp = clock.unix_timestamp;
    require!(
        msg.timeout_timestamp > current_timestamp,
        RouterError::InvalidTimeoutTimestamp
    );
    require!(
        msg.timeout_timestamp - current_timestamp <= MAX_TIMEOUT_DURATION,
        RouterError::InvalidTimeoutDuration
    );

    let sequence = client_sequence.next_sequence_send;
    client_sequence.next_sequence_send += 1;

    let counterparty_client_id = ctx.accounts.client.counterparty_info.client_id.clone();

    let packet = Packet {
        sequence,
        source_client: msg.source_client.clone(),
        dest_client: counterparty_client_id,
        timeout_timestamp: msg.timeout_timestamp,
        payloads: vec![msg.payload],
    };

    let commitment = ics24::packet_commitment_bytes32(&packet);
    packet_commitment.value = commitment;

    emit!(SendPacketEvent {
        client_id: msg.source_client,
        sequence,
        packet_data: packet.try_to_vec().unwrap(),
    });

    Ok(sequence)
}

#[event]
pub struct SendPacketEvent {
    pub client_id: String,
    pub sequence: u64,
    pub packet_data: Vec<u8>,
}
