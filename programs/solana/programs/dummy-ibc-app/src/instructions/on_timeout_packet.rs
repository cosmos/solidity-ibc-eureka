use crate::state::*;
use anchor_lang::prelude::*;

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
    /// CHECK: We trust the router to call us correctly
    pub router_program: AccountInfo<'info>,
}

pub fn on_timeout_packet(ctx: Context<OnTimeoutPacket>, msg: OnTimeoutPacketMsg) -> Result<()> {
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
