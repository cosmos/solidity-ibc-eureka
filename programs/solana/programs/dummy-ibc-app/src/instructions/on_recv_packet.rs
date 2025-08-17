use crate::errors::DummyIbcAppError;
use crate::state::*;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::set_return_data;

/// The ICS26 Router program ID that is authorized to call this instruction
pub const ICS26_ROUTER_ID: Pubkey = pubkey!("FRGF7cthWUvDvAHMUARUHFycyUK2VDUtBchmkwrz7hgx");

#[derive(Accounts)]
#[instruction(msg: OnRecvPacketMsg)]
pub struct OnRecvPacket<'info> {
    #[account(
        mut,
        seeds = [APP_STATE_SEED],
        bump
    )]
    pub app_state: Account<'info, DummyIbcAppState>,

    /// The IBC router program that's calling us
    /// CHECK: Verified to be the ICS26 Router program
    pub router_program: AccountInfo<'info>,
}

pub fn on_recv_packet(ctx: Context<OnRecvPacket>, msg: OnRecvPacketMsg) -> Result<()> {
    // Verify that the caller is the ICS26 Router program
    require_keys_eq!(
        ctx.accounts.router_program.key(),
        ICS26_ROUTER_ID,
        DummyIbcAppError::UnauthorizedCaller
    );

    let app_state = &mut ctx.accounts.app_state;

    // Increment packet received counter
    app_state.packets_received = app_state.packets_received.saturating_add(1);

    // Create acknowledgement (in real apps, this would contain meaningful data)
    // Return a simple acknowledgement for testing compatibility
    let acknowledgement = b"packet received".to_vec();

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
