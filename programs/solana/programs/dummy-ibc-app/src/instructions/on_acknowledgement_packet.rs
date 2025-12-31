use crate::{state::*, ICS26_ROUTER_ID};
use anchor_lang::prelude::*;

#[derive(Accounts)]
#[instruction(msg: OnAcknowledgementPacketMsg)]
pub struct OnAcknowledgementPacket<'info> {
    #[account(
        init_if_needed,
        payer = payer,
        space = 8 + DummyIbcAppState::INIT_SPACE,
        seeds = [IBCAppState::SEED, TRANSFER_PORT.as_bytes()],
        bump
    )]
    pub app_state: Account<'info, DummyIbcAppState>,

    /// The IBC router program that's calling us
    /// CHECK: Verified to be the ICS26 Router program
    pub router_program: AccountInfo<'info>,

    /// Instructions sysvar for CPI validation
    /// CHECK: Validated via address constraint
    #[account(address = anchor_lang::solana_program::sysvar::instructions::ID)]
    pub instruction_sysvar: AccountInfo<'info>,

    /// Payer for account creation if needed
    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn on_acknowledgement_packet(
    ctx: Context<OnAcknowledgementPacket>,
    msg: OnAcknowledgementPacketMsg,
) -> Result<()> {
    // Validate CPI caller using shared validation function
    solana_ibc_types::validate_cpi_caller(
        &ctx.accounts.instruction_sysvar,
        &ICS26_ROUTER_ID,
        &crate::ID,
    )?;

    let app_state = &mut ctx.accounts.app_state;

    // Initialize authority if this is the first time (account was just created)
    if app_state.authority == Pubkey::default() {
        app_state.authority = ctx.accounts.payer.key();
    }

    // Increment packet acknowledged counter
    app_state.packets_acknowledged = app_state.packets_acknowledged.saturating_add(1);

    // Emit event
    emit!(DummyAppPacketAcknowledged {
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
