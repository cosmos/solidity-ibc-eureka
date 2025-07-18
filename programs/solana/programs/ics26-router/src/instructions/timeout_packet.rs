use crate::errors::RouterError;
use crate::instructions::light_client_cpi::{
    verify_non_membership_cpi, LightClientVerification, NonMembershipMsg,
};
use crate::instructions::recv_packet::NoopEvent;
use crate::state::*;
use crate::utils::{construct_receipt_path, ics24_host};
use anchor_lang::prelude::*;

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

    // Client registry for light client lookup
    #[account(
        seeds = [CLIENT_REGISTRY_SEED, msg.packet.source_client.as_bytes()],
        bump,
        constraint = client_registry.active @ RouterError::ClientNotActive,
    )]
    pub client_registry: Account<'info, ClientRegistry>,

    // Light client verification accounts
    /// CHECK: Light client program, validated against client registry
    pub light_client_program: AccountInfo<'info>,

    /// CHECK: Client state account, owned by light client program
    pub client_state: AccountInfo<'info>,

    /// CHECK: Consensus state account, owned by light client program
    pub consensus_state: AccountInfo<'info>,
}

pub fn timeout_packet(ctx: Context<TimeoutPacket>, msg: MsgTimeoutPacket) -> Result<()> {
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

    // Verify non-membership proof on counterparty chain via light client
    let client_registry = &ctx.accounts.client_registry;
    let light_client_verification = LightClientVerification {
        light_client_program: ctx.accounts.light_client_program.clone(),
        client_state: ctx.accounts.client_state.clone(),
        consensus_state: ctx.accounts.consensus_state.clone(),
    };

    let receipt_path = construct_receipt_path(
        msg.packet.sequence,
        &msg.packet.payloads[0].source_port,
        &msg.packet.payloads[0].dest_port,
    );

    let non_membership_msg = NonMembershipMsg {
        height: msg.proof_height,
        delay_time_period: 0,
        delay_block_period: 0,
        proof: msg.proof_timeout.clone(),
        path: receipt_path,
    };

    let counterparty_timestamp = verify_non_membership_cpi(
        client_registry,
        &light_client_verification,
        non_membership_msg,
    )?;

    require!(
        counterparty_timestamp >= msg.packet.timeout_timestamp as u64,
        RouterError::InvalidTimeoutTimestamp
    );

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
