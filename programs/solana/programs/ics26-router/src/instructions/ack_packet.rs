use crate::errors::RouterError;
use crate::instructions::light_client_cpi::{verify_membership_cpi, LightClientVerification};
use crate::instructions::recv_packet::NoopEvent;
use crate::state::*;
use crate::utils::ics24;
use anchor_lang::prelude::*;
use solana_light_client_interface::MembershipMsg;

#[derive(Accounts)]
#[instruction(msg: MsgAckPacket)]
pub struct AckPacket<'info> {
    #[account(
        seeds = [ROUTER_STATE_SEED],
        bump
    )]
    pub router_state: Account<'info, RouterState>,

    #[account(
        seeds = [PORT_SEED, msg.packet.payloads[0].source_port.as_bytes()],
        bump
    )]
    pub port: Account<'info, Port>,

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

    // Client for light client lookup
    #[account(
        seeds = [CLIENT_SEED, msg.packet.source_client.as_bytes()],
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

pub fn ack_packet(ctx: Context<AckPacket>, msg: MsgAckPacket) -> Result<()> {
    // TODO: Support multi-payload packets #602
    let router_state = &ctx.accounts.router_state;
    let packet_commitment = &ctx.accounts.packet_commitment;

    require!(
        ctx.accounts.relayer.key() == router_state.authority,
        RouterError::UnauthorizedSender
    );

    require!(
        msg.packet.payloads.len() == 1,
        RouterError::MultiPayloadPacketNotSupported
    );

    // Verify acknowledgement proof on counterparty chain via light client
    let client = &ctx.accounts.client;
    let light_client_verification = LightClientVerification {
        light_client_program: ctx.accounts.light_client_program.clone(),
        client_state: ctx.accounts.client_state.clone(),
        consensus_state: ctx.accounts.consensus_state.clone(),
    };

    let ack_path = ics24::construct_ack_path(
        msg.packet.sequence,
        &msg.packet.payloads[0].source_port,
        &msg.packet.payloads[0].dest_port,
    );

    let membership_msg = MembershipMsg {
        height: msg.proof_height,
        delay_time_period: 0,
        delay_block_period: 0,
        proof: msg.proof_acked.clone(),
        path: ack_path,
        value: msg.acknowledgement.clone(),
    };

    verify_membership_cpi(client, &light_client_verification, membership_msg)?;

    let expected_commitment = ics24::packet_commitment_bytes32(&msg.packet);
    if packet_commitment.value != expected_commitment {
        // No-op case - commitment doesn't exist or mismatch
        emit!(NoopEvent {});
        return Ok(());
    }

    // TODO: CPI to IBC app's onAcknowledgementPacket

    // The account will be closed automatically by Anchor due to close = payer

    emit!(AckPacketEvent {
        client_id: msg.packet.source_client.clone(),
        sequence: msg.packet.sequence,
        packet_data: msg.packet.try_to_vec().unwrap(),
        acknowledgement: msg.acknowledgement.clone(),
    });

    Ok(())
}

#[event]
pub struct AckPacketEvent {
    pub client_id: String,
    pub sequence: u64,
    pub packet_data: Vec<u8>,
    pub acknowledgement: Vec<u8>,
}
