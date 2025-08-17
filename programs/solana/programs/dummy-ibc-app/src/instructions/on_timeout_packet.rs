use crate::errors::DummyIbcAppError;
use crate::state::*;
use anchor_lang::prelude::*;

/// The ICS26 Router program ID that is authorized to call this instruction
pub const ICS26_ROUTER_ID: Pubkey = pubkey!("FRGF7cthWUvDvAHMUARUHFycyUK2VDUtBchmkwrz7hgx");

#[derive(Accounts)]
#[instruction(msg: OnTimeoutPacketMsg)]
pub struct OnTimeoutPacket<'info> {
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

pub fn on_timeout_packet(ctx: Context<OnTimeoutPacket>, msg: OnTimeoutPacketMsg) -> Result<()> {
    // Verify that the caller is the ICS26 Router program
    require_keys_eq!(
        ctx.accounts.router_program.key(),
        ICS26_ROUTER_ID,
        DummyIbcAppError::UnauthorizedCaller
    );

    let app_state = &mut ctx.accounts.app_state;

    // Increment packet timed out counter
    app_state.packets_timed_out = app_state.packets_timed_out.saturating_add(1);

    // Emit event
    emit!(PacketTimedOut {
        source_client: msg.source_client.clone(),
        dest_client: msg.dest_client.clone(),
        sequence: msg.sequence,
    });

    msg!(
        "Dummy IBC App: Timed out packet from {} to {} (seq: {}), total timed out: {}",
        msg.source_client,
        msg.dest_client,
        msg.sequence,
        app_state.packets_timed_out
    );

    Ok(())
}
