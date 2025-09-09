use crate::{errors::DummyIbcAppError, state::*, ICS26_ROUTER_ID};
use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::set_return_data;

const SUCCESSFUL_ACKNOWLEDGEMENT_JSON: &[u8] = br#"{"result": "AQ=="}"#;

#[derive(Accounts)]
#[instruction(msg: OnRecvPacketMsg)]
pub struct OnRecvPacket<'info> {
    #[account(
        init_if_needed,
        payer = payer,
        space = 8 + DummyIbcAppState::INIT_SPACE,
        seeds = [APP_STATE_SEED, TRANSFER_PORT.as_bytes()],
        bump
    )]
    pub app_state: Account<'info, DummyIbcAppState>,

    /// The IBC router program that's calling us
    /// CHECK: Verified to be the ICS26 Router program
    pub router_program: AccountInfo<'info>,

    /// Payer for account creation if needed
    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn on_recv_packet(ctx: Context<OnRecvPacket>, msg: OnRecvPacketMsg) -> Result<()> {
    require_keys_eq!(
        ctx.accounts.router_program.key(),
        ICS26_ROUTER_ID,
        DummyIbcAppError::UnauthorizedCaller
    );

    let app_state = &mut ctx.accounts.app_state;

    // Initialize authority if this is the first time (account was just created)
    if app_state.authority == Pubkey::default() {
        app_state.authority = ctx.accounts.payer.key();
    }

    // Increment packet received counter
    app_state.packets_received = app_state.packets_received.saturating_add(1);

    // Create acknowledgement in ICS-20 format: {"result": "AQ=="} where "AQ==" is base64 for []byte{1}
    // This indicates successful packet processing
    let acknowledgement = SUCCESSFUL_ACKNOWLEDGEMENT_JSON.to_vec();

    // Return acknowledgement data to the router
    set_return_data(&acknowledgement);

    // Emit event
    emit!(PacketReceived {
        source_client: msg.source_client.clone(),
        dest_client: msg.dest_client.clone(),
        sequence: msg.sequence,
        acknowledgement,
    });

    msg!(
        "Dummy IBC App: Received packet from {} to {} (seq: {}), total received: {}",
        msg.source_client,
        msg.dest_client,
        msg.sequence,
        app_state.packets_received
    );

    Ok(())
}
