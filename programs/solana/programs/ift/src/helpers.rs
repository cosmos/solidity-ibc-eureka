//! Shared helper functions for IFT operations

use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, MintTo, Token, TokenAccount};

use crate::constants::{MINT_AUTHORITY_SEED, SECONDS_PER_DAY};
use crate::errors::IFTError;
use crate::state::IFTAppState;

/// Mint tokens to an account using the IFT mint authority PDA
pub fn mint_to_account<'info>(
    mint: &Account<'info, Mint>,
    to: &Account<'info, TokenAccount>,
    mint_authority: &AccountInfo<'info>,
    mint_authority_bump: u8,
    token_program: &Program<'info, Token>,
    amount: u64,
) -> Result<()> {
    let mint_key = mint.key();
    let seeds = &[
        MINT_AUTHORITY_SEED,
        mint_key.as_ref(),
        &[mint_authority_bump],
    ];
    let signer_seeds = &[&seeds[..]];

    let mint_accounts = MintTo {
        mint: mint.to_account_info(),
        to: to.to_account_info(),
        authority: mint_authority.to_account_info(),
    };
    let mint_ctx =
        CpiContext::new_with_signer(token_program.to_account_info(), mint_accounts, signer_seeds);
    token::mint_to(mint_ctx, amount)
}

const fn current_day(clock: &Clock) -> u64 {
    if clock.unix_timestamp < 0 {
        return 0;
    }
    clock.unix_timestamp as u64 / SECONDS_PER_DAY
}

#[allow(clippy::missing_const_for_fn)]
fn maybe_reset_day(app_state: &mut IFTAppState, clock: &Clock) {
    let today = current_day(clock);
    if app_state.rate_limit_day != today {
        app_state.rate_limit_day = today;
        app_state.rate_limit_daily_usage = 0;
    }
}

/// Update mint rate limit usage for `ift_mint`.
/// Resets usage on new day, adds amount, returns error if limit exceeded.
pub fn check_and_update_mint_rate_limit(
    app_state: &mut IFTAppState,
    amount: u64,
    clock: &Clock,
) -> Result<()> {
    if app_state.daily_mint_limit == 0 {
        return Ok(());
    }
    maybe_reset_day(app_state, clock);
    let new_usage = app_state
        .rate_limit_daily_usage
        .checked_add(amount)
        .ok_or_else(|| error!(IFTError::MintRateLimitExceeded))?;
    require!(
        new_usage <= app_state.daily_mint_limit,
        IFTError::MintRateLimitExceeded
    );
    app_state.rate_limit_daily_usage = new_usage;
    Ok(())
}

/// Reduce mint rate limit usage for `ift_transfer` (burn).
pub fn reduce_mint_rate_limit_usage(app_state: &mut IFTAppState, amount: u64, clock: &Clock) {
    if app_state.daily_mint_limit == 0 {
        return;
    }
    maybe_reset_day(app_state, clock);
    app_state.rate_limit_daily_usage = app_state.rate_limit_daily_usage.saturating_sub(amount);
}

/// Increase mint rate limit usage for `claim_refund` (refund re-mints).
/// Does not check the limit -- refunds must never be blocked.
pub fn increase_mint_rate_limit_usage(app_state: &mut IFTAppState, amount: u64, clock: &Clock) {
    if app_state.daily_mint_limit == 0 {
        return;
    }
    maybe_reset_day(app_state, clock);
    app_state.rate_limit_daily_usage = app_state.rate_limit_daily_usage.saturating_add(amount);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AccountVersion;

    fn make_app_state(limit: u64, day: u64, usage: u64) -> IFTAppState {
        IFTAppState {
            version: AccountVersion::V1,
            bump: 0,
            mint: Pubkey::new_unique(),
            mint_authority_bump: 0,
            admin: Pubkey::new_unique(),
            gmp_program: Pubkey::new_unique(),
            daily_mint_limit: limit,
            rate_limit_day: day,
            rate_limit_daily_usage: usage,
            paused: false,
            _reserved: [0; 128],
        }
    }

    fn make_clock(unix_timestamp: i64) -> Clock {
        Clock {
            slot: 0,
            epoch_start_timestamp: 0,
            epoch: 0,
            leader_schedule_epoch: 0,
            unix_timestamp,
        }
    }

    #[test]
    fn test_check_and_update_no_limit() {
        let mut state = make_app_state(0, 0, 0);
        let clock = make_clock(100_000);
        assert!(check_and_update_mint_rate_limit(&mut state, 999_999, &clock).is_ok());
        assert_eq!(state.rate_limit_daily_usage, 0);
    }

    #[test]
    fn test_check_and_update_within_limit() {
        let day = 100_000i64 / SECONDS_PER_DAY as i64;
        let mut state = make_app_state(1000, day as u64, 0);
        let clock = make_clock(100_000);
        assert!(check_and_update_mint_rate_limit(&mut state, 500, &clock).is_ok());
        assert_eq!(state.rate_limit_daily_usage, 500);
    }

    #[test]
    fn test_check_and_update_at_limit() {
        let day = 100_000i64 / SECONDS_PER_DAY as i64;
        let mut state = make_app_state(1000, day as u64, 500);
        let clock = make_clock(100_000);
        assert!(check_and_update_mint_rate_limit(&mut state, 500, &clock).is_ok());
        assert_eq!(state.rate_limit_daily_usage, 1000);
    }

    #[test]
    fn test_check_and_update_exceeds_limit() {
        let day = 100_000i64 / SECONDS_PER_DAY as i64;
        let mut state = make_app_state(1000, day as u64, 500);
        let clock = make_clock(100_000);
        assert!(check_and_update_mint_rate_limit(&mut state, 501, &clock).is_err());
    }

    #[test]
    fn test_check_and_update_resets_on_new_day() {
        let mut state = make_app_state(1000, 0, 999);
        let clock = make_clock(SECONDS_PER_DAY as i64); // day 1
        assert!(check_and_update_mint_rate_limit(&mut state, 500, &clock).is_ok());
        assert_eq!(state.rate_limit_daily_usage, 500);
        assert_eq!(state.rate_limit_day, 1);
    }

    #[test]
    fn test_reduce_usage() {
        let mut state = make_app_state(1000, 1, 800);
        let clock = make_clock(SECONDS_PER_DAY as i64);
        reduce_mint_rate_limit_usage(&mut state, 300, &clock);
        assert_eq!(state.rate_limit_daily_usage, 500);
    }

    #[test]
    fn test_reduce_usage_saturates_to_zero() {
        let mut state = make_app_state(1000, 1, 100);
        let clock = make_clock(SECONDS_PER_DAY as i64);
        reduce_mint_rate_limit_usage(&mut state, 500, &clock);
        assert_eq!(state.rate_limit_daily_usage, 0);
    }

    #[test]
    fn test_reduce_usage_no_limit() {
        let mut state = make_app_state(0, 0, 0);
        let clock = make_clock(100_000);
        reduce_mint_rate_limit_usage(&mut state, 500, &clock);
        assert_eq!(state.rate_limit_daily_usage, 0);
    }

    #[test]
    fn test_increase_usage_no_assert() {
        let mut state = make_app_state(1000, 1, 900);
        let clock = make_clock(SECONDS_PER_DAY as i64);
        increase_mint_rate_limit_usage(&mut state, 200, &clock);
        assert_eq!(state.rate_limit_daily_usage, 1100); // exceeds limit, no error
    }

    #[test]
    fn test_increase_usage_no_limit() {
        let mut state = make_app_state(0, 0, 0);
        let clock = make_clock(100_000);
        increase_mint_rate_limit_usage(&mut state, 500, &clock);
        assert_eq!(state.rate_limit_daily_usage, 0);
    }

    #[test]
    fn test_increase_usage_saturates_on_overflow() {
        let mut state = make_app_state(1000, 1, u64::MAX - 10);
        let clock = make_clock(SECONDS_PER_DAY as i64);
        increase_mint_rate_limit_usage(&mut state, 100, &clock);
        assert_eq!(state.rate_limit_daily_usage, u64::MAX);
    }

    #[test]
    fn test_at_limit_then_one_more_rejected() {
        let day = 1u64;
        let mut state = make_app_state(1000, day, 1000);
        let clock = make_clock(SECONDS_PER_DAY as i64);
        assert!(check_and_update_mint_rate_limit(&mut state, 1, &clock).is_err());
        assert_eq!(state.rate_limit_daily_usage, 1000);
    }

    #[test]
    fn test_mint_burn_mint_cycle() {
        let day = 1u64;
        let clock = make_clock(SECONDS_PER_DAY as i64);
        let mut state = make_app_state(1000, day, 0);

        // Mint 800
        assert!(check_and_update_mint_rate_limit(&mut state, 800, &clock).is_ok());
        assert_eq!(state.rate_limit_daily_usage, 800);

        // Burn 300
        reduce_mint_rate_limit_usage(&mut state, 300, &clock);
        assert_eq!(state.rate_limit_daily_usage, 500);

        // Mint 500 more — should succeed (500 + 500 = 1000)
        assert!(check_and_update_mint_rate_limit(&mut state, 500, &clock).is_ok());
        assert_eq!(state.rate_limit_daily_usage, 1000);

        // Mint 1 more — should fail
        assert!(check_and_update_mint_rate_limit(&mut state, 1, &clock).is_err());
    }

    #[test]
    fn test_mint_burn_refund_cycle() {
        let day = 1u64;
        let clock = make_clock(SECONDS_PER_DAY as i64);
        let mut state = make_app_state(1000, day, 0);

        // Mint to limit
        assert!(check_and_update_mint_rate_limit(&mut state, 1000, &clock).is_ok());

        // Burn 500 (transfer out)
        reduce_mint_rate_limit_usage(&mut state, 500, &clock);
        assert_eq!(state.rate_limit_daily_usage, 500);

        // Refund 500 (transfer failed)
        increase_mint_rate_limit_usage(&mut state, 500, &clock);
        assert_eq!(state.rate_limit_daily_usage, 1000);

        // New mint should fail — refund restored usage
        assert!(check_and_update_mint_rate_limit(&mut state, 1, &clock).is_err());
    }

    #[test]
    fn test_reduce_resets_on_new_day() {
        let mut state = make_app_state(1000, 0, 800);
        let clock = make_clock(SECONDS_PER_DAY as i64); // day 1
        reduce_mint_rate_limit_usage(&mut state, 300, &clock);
        // Day reset to 0, then saturating_sub(0, 300) = 0
        assert_eq!(state.rate_limit_day, 1);
        assert_eq!(state.rate_limit_daily_usage, 0);
    }

    #[test]
    fn test_negative_timestamp_clamps_to_day_zero() {
        let mut state = make_app_state(1000, 0, 0);
        let clock = make_clock(-86400); // negative timestamp
        assert!(check_and_update_mint_rate_limit(&mut state, 500, &clock).is_ok());
        assert_eq!(state.rate_limit_day, 0);
        assert_eq!(state.rate_limit_daily_usage, 500);
    }

    #[test]
    fn test_checked_add_overflow_returns_error() {
        let day = 1u64;
        let mut state = make_app_state(u64::MAX, day, u64::MAX - 100);
        let clock = make_clock(SECONDS_PER_DAY as i64);
        assert!(check_and_update_mint_rate_limit(&mut state, 200, &clock).is_err());
        assert_eq!(state.rate_limit_daily_usage, u64::MAX - 100); // unchanged
    }

    #[test]
    fn test_increase_resets_on_new_day() {
        let mut state = make_app_state(1000, 0, 800);
        let clock = make_clock(SECONDS_PER_DAY as i64); // day 1
        increase_mint_rate_limit_usage(&mut state, 200, &clock);
        // Day reset to 0, then 0 + 200 = 200
        assert_eq!(state.rate_limit_day, 1);
        assert_eq!(state.rate_limit_daily_usage, 200);
    }
}
