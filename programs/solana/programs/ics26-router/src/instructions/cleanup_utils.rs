use crate::errors::RouterError;
use anchor_lang::prelude::*;

/// Close an account and reclaim rent to the recipient
///
/// Returns the amount of lamports reclaimed
///
/// # Errors
///
/// * `RouterError::ArithmeticOverflow` - If adding lamports to recipient would overflow
/// * `ProgramError` - If borrowing account lamports or data fails
pub fn close_account<'info>(
    account: &AccountInfo<'info>,
    recipient: &AccountInfo<'info>,
) -> Result<u64> {
    let lamports_to_reclaim = account.lamports();

    // Transfer lamports to recipient
    **recipient.lamports.borrow_mut() = recipient
        .lamports()
        .checked_add(lamports_to_reclaim)
        .ok_or(RouterError::ArithmeticOverflow)?;
    **account.lamports.borrow_mut() = 0;

    // Clear account data
    let mut data = account.try_borrow_mut_data()?;
    data.fill(0);

    Ok(lamports_to_reclaim)
}
