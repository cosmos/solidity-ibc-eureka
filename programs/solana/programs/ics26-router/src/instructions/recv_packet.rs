use crate::errors::IbcRouterError;
use crate::state::*;
use crate::utils::ics24_host;
use anchor_lang::prelude::*;

#[derive(Accounts)]
#[instruction(msg: MsgRecvPacket)]
pub struct RecvPacket<'info> {
    #[account(
        seeds = [ROUTER_STATE_SEED],
        bump
    )]
    pub router_state: Account<'info, RouterState>,

    #[account(
        seeds = [PORT_REGISTRY_SEED, msg.packet.payloads[0].dest_port.as_bytes()],
        bump
    )]
    pub port_registry: Account<'info, PortRegistry>,

    #[account(
        mut,
        seeds = [CLIENT_SEQUENCE_SEED, msg.packet.dest_client.as_bytes()],
        bump
    )]
    pub client_sequence: Account<'info, ClientSequence>,

    #[account(
        init_if_needed,
        payer = payer,
        space = 8 + Commitment::INIT_SPACE, // discriminator + receipt
        seeds = [
            PACKET_RECEIPT_SEED,
            msg.packet.dest_client.as_bytes(),
            &msg.packet.sequence.to_le_bytes()
        ],
        bump
    )]
    pub packet_receipt: Account<'info, Commitment>,

    #[account(
        init,
        payer = payer,
        space = 8 + Commitment::INIT_SPACE, // discriminator + commitment
        seeds = [
            PACKET_ACK_SEED,
            msg.packet.dest_client.as_bytes(),
            &msg.packet.sequence.to_le_bytes()
        ],
        bump
    )]
    pub packet_ack: Account<'info, Commitment>,

    pub relayer: Signer<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,

    pub clock: Sysvar<'info, Clock>,
    // TODO: Add light client accounts for proof verification
}

pub fn recv_packet(ctx: Context<RecvPacket>, msg: MsgRecvPacket) -> Result<()> {
    let router_state = &ctx.accounts.router_state;
    let packet_receipt = &mut ctx.accounts.packet_receipt;
    let packet_ack = &mut ctx.accounts.packet_ack;
    let clock = &ctx.accounts.clock;

    // Check authority (relayer must be authorized)
    require!(
        ctx.accounts.relayer.key() == router_state.authority,
        IbcRouterError::UnauthorizedSender
    );

    // Multi-payload check
    require!(
        msg.packet.payloads.len() == 1,
        IbcRouterError::MultiPayloadPacketNotSupported
    );

    // Validate timeout
    let current_timestamp = clock.unix_timestamp;
    require!(
        msg.packet.timeout_timestamp > current_timestamp,
        IbcRouterError::InvalidTimeoutTimestamp
    );

    // TODO: Verify counterparty client ID
    // This would normally check against stored client info

    // TODO: Verify merkle proof via CPI to light client
    // For now, we'll skip the actual verification

    // Check if receipt already exists (no-op case)
    let receipt_commitment = ics24_host::packet_receipt_commitment_bytes32(&msg.packet);
    if packet_receipt.value == receipt_commitment {
        emit!(NoopEvent {});
        return Ok(());
    }

    // Set packet receipt
    packet_receipt.value = receipt_commitment;

    // TODO: CPI to IBC app's onRecvPacket
    // For now, we'll create a simple acknowledgement
    let ack_data = b"packet received".to_vec();

    // Store acknowledgement commitment
    let acks = vec![ack_data.clone()];
    let ack_commitment = ics24_host::packet_acknowledgement_commitment_bytes32(&acks)?;
    packet_ack.value = ack_commitment;

    // Emit event
    emit!(WriteAcknowledgementEvent {
        client_id: msg.packet.dest_client.clone(),
        sequence: msg.packet.sequence,
        packet_data: msg.packet.try_to_vec().unwrap(),
        acknowledgements: acks,
    });

    Ok(())
}

#[event]
pub struct WriteAcknowledgementEvent {
    pub client_id: String,
    pub sequence: u64,
    pub packet_data: Vec<u8>,
    pub acknowledgements: Vec<Vec<u8>>,
}

#[event]
pub struct NoopEvent {}
