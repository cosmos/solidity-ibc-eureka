use crate::state::AccessManager;
use crate::types::AccessManagerError;
use anchor_lang::prelude::*;

/// Helper function to check if a signer has a required role
///
/// This function should be used by other programs to verify access control.
///
/// # Arguments
/// * `access_manager_account` - The access manager account info
/// * `role_id` - The role ID to check (e.g., `RELAYER_ROLE`, `PAUSER_ROLE`)
/// * `signer` - The signer pubkey to check
///
/// # Returns
/// * `Ok(())` if the signer has the required role
/// * `Err(...)` if the signer does not have the role
///
/// # Example
/// ```ignore
/// access_manager::require_role(
///     &ctx.accounts.access_manager,
///     solana_ibc_types::roles::RELAYER_ROLE,
///     &ctx.accounts.relayer.key()
/// )?;
/// ```
pub fn require_role(
    access_manager_account: &AccountInfo,
    role_id: u64,
    signer: &Pubkey,
) -> Result<()> {
    let access_manager_data = access_manager_account.try_borrow_data()?;
    let access_manager: AccessManager =
        anchor_lang::AccountDeserialize::try_deserialize(&mut &access_manager_data[..])?;

    require!(
        access_manager.has_role(role_id, signer),
        AccessManagerError::Unauthorized
    );

    Ok(())
}
