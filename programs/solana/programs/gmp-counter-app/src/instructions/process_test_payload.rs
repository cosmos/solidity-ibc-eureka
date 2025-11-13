use crate::state::*;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::set_return_data;

#[derive(Accounts)]
pub struct ProcessTestPayload<'info> {
    #[account(
        seeds = [CounterAppState::SEED],
        bump = app_state.bump
    )]
    pub app_state: Account<'info, CounterAppState>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn process_test_payload<'info>(
    ctx: Context<'_, '_, '_, 'info, ProcessTestPayload<'info>>,
    data: Vec<u8>,
) -> Result<()> {
    let num_remaining_accounts = ctx.remaining_accounts.len();

    msg!(
        "GMP Counter App: Processed test payload - data size: {} bytes, remaining accounts: {}",
        data.len(),
        num_remaining_accounts
    );

    set_return_data(b"ok");

    Ok(())
}
