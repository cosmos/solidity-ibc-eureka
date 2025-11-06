use crate::{errors::DummyIbcAppError, state::*};
use anchor_lang::prelude::*;
use ics26_router::cpi as router_cpi;
use ics26_router::program::Ics26Router;
use ics26_router::{
    cpi::accounts::SendPacket as RouterSendPacket,
    state::{Client, ClientSequence, IBCApp, MsgSendPacket, RouterState},
};
use solana_ibc_types::Payload;

/// Message for sending an arbitrary packet via IBC
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct SendPacketMsg {
    /// Source client ID for the destination chain
    pub source_client: String,
    /// Source port (e.g., "transfer", "oracle", etc.)
    pub source_port: String,
    /// Destination port
    pub dest_port: String,
    /// Version string for the packet
    pub version: String,
    /// Encoding format (e.g., "json", "protobuf")
    pub encoding: String,
    /// Arbitrary packet data
    pub packet_data: Vec<u8>,
    /// Timeout timestamp (Unix timestamp in seconds)
    pub timeout_timestamp: i64,
}

#[derive(Accounts)]
#[instruction(msg: SendPacketMsg)]
pub struct SendPacket<'info> {
    #[account(
        mut,
        seeds = [IBCAppState::SEED, TRANSFER_PORT.as_bytes()],
        bump
    )]
    pub app_state: Account<'info, DummyIbcAppState>,

    /// User sending the packet
    #[account(mut)]
    pub user: Signer<'info>,

    // Router CPI accounts
    #[account(
        seeds = [RouterState::SEED],
        bump,
        seeds::program = router_program
    )]
    pub router_state: Account<'info, RouterState>,

    #[account(
        seeds = [IBCApp::SEED, msg.source_port.as_bytes()],
        bump,
        seeds::program = router_program
    )]
    pub ibc_app: Account<'info, IBCApp>,

    #[account(
        mut,
        seeds = [ClientSequence::SEED, msg.source_client.as_bytes()],
        bump,
        seeds::program = router_program
    )]
    pub client_sequence: Account<'info, ClientSequence>,

    /// Will be created by the router
    /// CHECK: PDA will be validated by router program
    #[account(mut)]
    pub packet_commitment: AccountInfo<'info>,

    #[account(
        seeds = [Client::SEED, msg.source_client.as_bytes()],
        bump,
        seeds::program = router_program
    )]
    pub client: Account<'info, Client>,

    /// Router program for CPI
    pub router_program: Program<'info, Ics26Router>,

    pub system_program: Program<'info, System>,

    /// PDA that acts as the router caller for CPI calls to the IBC router.
    #[account(
        seeds = [solana_ibc_types::RouterCaller::SEED],
        bump
    )]
    pub router_caller: SystemAccount<'info>,
}

pub fn send_packet(ctx: Context<SendPacket>, msg: SendPacketMsg) -> Result<()> {
    let app_state = &mut ctx.accounts.app_state;
    // Get clock directly via syscall
    let clock = Clock::get()?;

    // Validate timeout
    if msg.timeout_timestamp <= clock.unix_timestamp {
        return Err(error!(DummyIbcAppError::InvalidPacketData));
    }

    let payload = Payload {
        source_port: msg.source_port.clone(),
        dest_port: msg.dest_port.clone(),
        version: msg.version,
        encoding: msg.encoding,
        value: msg.packet_data.clone(),
    };

    // Call router via CPI to send packet
    let router_msg = MsgSendPacket {
        source_client: msg.source_client.clone(),
        timeout_timestamp: msg.timeout_timestamp,
        payload,
    };

    let cpi_accounts = RouterSendPacket {
        router_state: ctx.accounts.router_state.to_account_info(),
        ibc_app: ctx.accounts.ibc_app.to_account_info(),
        client_sequence: ctx.accounts.client_sequence.to_account_info(),
        packet_commitment: ctx.accounts.packet_commitment.to_account_info(),
        app_caller: ctx.accounts.router_caller.to_account_info(),
        payer: ctx.accounts.user.to_account_info(),
        system_program: ctx.accounts.system_program.to_account_info(),
        client: ctx.accounts.client.to_account_info(),
    };

    // Create PDA signer for CPI call
    let seeds = &[
        solana_ibc_types::RouterCaller::SEED,
        &[ctx.bumps.router_caller],
    ];
    let signer_seeds = &[&seeds[..]];

    let cpi_ctx = CpiContext::new_with_signer(
        ctx.accounts.router_program.to_account_info(),
        cpi_accounts,
        signer_seeds,
    );
    let sequence_result = router_cpi::send_packet(cpi_ctx, router_msg)?;
    let sequence = sequence_result.get();

    // Update app state - track packets sent
    app_state.packets_sent = app_state.packets_sent.saturating_add(1);

    // Emit event for tracking
    emit!(PacketSent {
        sequence,
        source_client: msg.source_client.clone(),
        source_port: msg.source_port.clone(),
        dest_port: msg.dest_port.clone(),
        data_length: msg.packet_data.len() as u64,
    });

    msg!(
        "Dummy app sent packet: {} -> {} (seq: {}, {} bytes)",
        msg.source_port,
        msg.dest_port,
        sequence,
        msg.packet_data.len()
    );

    Ok(())
}

#[event]
pub struct PacketSent {
    pub sequence: u64,
    pub source_client: String,
    pub source_port: String,
    pub dest_port: String,
    pub data_length: u64,
}
