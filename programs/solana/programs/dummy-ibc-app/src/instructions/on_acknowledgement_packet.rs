use crate::errors::DummyIbcAppError;
use crate::state::*;
use anchor_lang::prelude::*;

/// The ICS26 Router program ID that is authorized to call this instruction
pub const ICS26_ROUTER_ID: Pubkey = pubkey!("FRGF7cthWUvDvAHMUARUHFycyUK2VDUtBchmkwrz7hgx");

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
    /// CHECK: Verified to be the ICS26 Router program
    pub router_program: AccountInfo<'info>,
}

pub fn on_acknowledgement_packet(
    ctx: Context<OnAcknowledgementPacket>,
    msg: OnAcknowledgementPacketMsg,
) -> Result<()> {
    // Verify that the caller is the ICS26 Router program
    require_keys_eq!(
        ctx.accounts.router_program.key(),
        ICS26_ROUTER_ID,
        DummyIbcAppError::UnauthorizedCaller
    );

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
