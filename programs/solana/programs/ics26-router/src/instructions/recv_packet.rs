use crate::errors::RouterError;
use crate::instructions::light_client_cpi::{verify_membership_cpi, LightClientVerification};
use crate::state::*;
use crate::utils::ics24;
use anchor_lang::prelude::*;
use solana_light_client_interface::MembershipMsg;

#[derive(Accounts)]
#[instruction(msg: MsgRecvPacket)]
pub struct RecvPacket<'info> {
    #[account(
        seeds = [ROUTER_STATE_SEED],
        bump
    )]
    pub router_state: Account<'info, RouterState>,

    #[account(
        seeds = [PORT_SEED, msg.packet.payloads[0].dest_port.as_bytes()],
        bump
    )]
    pub port: Account<'info, Port>,

    #[account(
        mut,
        seeds = [CLIENT_SEQUENCE_SEED, msg.packet.dest_client.as_bytes()],
        bump
    )]
    pub client_sequence: Account<'info, ClientSequence>,

    #[account(
        init_if_needed,
        payer = payer,
        space = 8 + Commitment::INIT_SPACE,
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
        space = 8 + Commitment::INIT_SPACE,
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

    // Client for light client lookup
    #[account(
        seeds = [CLIENT_SEED, msg.packet.dest_client.as_bytes()],
        bump,
        constraint = client.active @ RouterError::ClientNotActive,
    )]
    pub client: Account<'info, Client>,

    // Light client verification accounts
    /// CHECK: Light client program, validated against client registry
    pub light_client_program: AccountInfo<'info>,

    /// CHECK: Client state account, owned by light client program
    pub client_state: AccountInfo<'info>,

    /// CHECK: Consensus state account, owned by light client program
    pub consensus_state: AccountInfo<'info>,
}

pub fn recv_packet(ctx: Context<RecvPacket>, msg: MsgRecvPacket) -> Result<()> {
    // TODO: Support multi-payload packets #602
    let router_state = &ctx.accounts.router_state;
    let packet_receipt = &mut ctx.accounts.packet_receipt;
    let packet_ack = &mut ctx.accounts.packet_ack;
    let clock = &ctx.accounts.clock;

    // Check authority (relayer must be authorized)
    require!(
        ctx.accounts.relayer.key() == router_state.authority,
        RouterError::UnauthorizedSender
    );

    // Multi-payload check
    require!(
        msg.packet.payloads.len() == 1,
        RouterError::MultiPayloadPacketNotSupported
    );

    // Validate timeout
    let current_timestamp = clock.unix_timestamp;
    require!(
        msg.packet.timeout_timestamp > current_timestamp,
        RouterError::InvalidTimeoutTimestamp
    );

    // Verify packet commitment on counterparty chain via light client
    let client = &ctx.accounts.client;
    let light_client_verification = LightClientVerification {
        light_client_program: ctx.accounts.light_client_program.clone(),
        client_state: ctx.accounts.client_state.clone(),
        consensus_state: ctx.accounts.consensus_state.clone(),
    };

    let commitment_path = ics24::construct_commitment_path(
        msg.packet.sequence,
        &msg.packet.payloads[0].source_port,
        &msg.packet.payloads[0].dest_port,
    );

    let expected_commitment = ics24::packet_commitment_bytes32(&msg.packet);

    // Verify membership proof via CPI to light client
    let membership_msg = MembershipMsg {
        height: msg.proof_height,
        delay_time_period: 0,
        delay_block_period: 0,
        proof: msg.proof_commitment.clone(),
        path: commitment_path,
        value: expected_commitment.to_vec(),
    };

    verify_membership_cpi(client, &light_client_verification, membership_msg)?;

    // Check if receipt already exists (no-op case)
    let receipt_commitment = ics24::packet_receipt_commitment_bytes32(&msg.packet);
    if packet_receipt.value == receipt_commitment {
        emit!(NoopEvent {});
        return Ok(());
    }

    packet_receipt.value = receipt_commitment;

    // TODO: CPI to IBC app's onRecvPacket
    // For now, we'll create a simple acknowledgement
    let ack_data = b"packet received".to_vec();

    let acks = vec![ack_data.clone()];
    let ack_commitment = ics24::packet_acknowledgement_commitment_bytes32(&acks)?;
    packet_ack.value = ack_commitment;

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
