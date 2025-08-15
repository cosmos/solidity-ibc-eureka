use crate::state::*;
use anchor_lang::prelude::*;

#[derive(Accounts)]
#[instruction(msg: OnAcknowledgementPacketMsg)]
pub struct OnAcknowledgementPacket<'info> {
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

pub fn on_acknowledgement_packet(
    ctx: Context<OnAcknowledgementPacket>,
    msg: OnAcknowledgementPacketMsg,
) -> Result<()> {
    let app_state = &mut ctx.accounts.app_state;

    // Increment packet acknowledged counter
    app_state.packets_acknowledged = app_state.packets_acknowledged.saturating_add(1);

    // Emit event
    emit!(PacketAcknowledged {
        source_client: msg.source_client.clone(),
        dest_client: msg.dest_client.clone(),
        sequence: msg.sequence,
        acknowledgement: msg.acknowledgement.clone(),
    });

    msg!(
        "Dummy IBC App: Acknowledged packet from {} to {} (seq: {}), ack: {:?}, total acknowledged: {}",
        msg.source_client,
        msg.dest_client,
        msg.sequence,
        String::from_utf8_lossy(&msg.acknowledgement),
        app_state.packets_acknowledged
    );

    Ok(())
}
