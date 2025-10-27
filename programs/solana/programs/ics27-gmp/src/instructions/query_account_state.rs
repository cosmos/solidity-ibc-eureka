use crate::state::AccountState;
use anchor_lang::prelude::*;

/// Query GMP account state
///
/// This instruction exists to expose `AccountState` in the IDL for client code generation.
/// It returns the current state of a GMP account including nonce, execution count, etc.
#[derive(Accounts)]
pub struct QueryAccountState<'info> {
    /// The GMP account state being queried
    /// CHECK: This is a read-only query instruction. The account is validated
    /// by the caller who must derive the correct PDA using `AccountState::derive_address`
    pub account_state: Account<'info, AccountState>,
}

pub fn query_account_state(ctx: Context<QueryAccountState>) -> Result<AccountState> {
    let account_state = &ctx.accounts.account_state;

    Ok(AccountState {
        client_id: account_state.client_id.clone(),
        sender: account_state.sender.clone(),
        salt: account_state.salt.clone(),
        nonce: account_state.nonce,
        created_at: account_state.created_at,
        last_executed_at: account_state.last_executed_at,
        execution_count: account_state.execution_count,
        bump: account_state.bump,
    })
}
