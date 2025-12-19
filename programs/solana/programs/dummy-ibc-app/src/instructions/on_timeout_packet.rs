use crate::{state::*, ICS26_ROUTER_ID};
use anchor_lang::prelude::*;

#[derive(Accounts)]
#[instruction(msg: OnTimeoutPacketMsg)]
pub struct OnTimeoutPacket<'info> {
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

    /// Escrow account that holds SOL (funds remain in escrow on timeout)
    /// CHECK: PDA derived from `source_client`
    #[account(
        mut,
        seeds = [DummyIbcAppState::ESCROW_SEED, msg.source_client.as_bytes()],
        bump
    )]
    pub escrow_account: Option<AccountInfo<'info>>,

    /// Payer for account creation if needed
    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn on_timeout_packet(ctx: Context<OnTimeoutPacket>, msg: OnTimeoutPacketMsg) -> Result<()> {
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
