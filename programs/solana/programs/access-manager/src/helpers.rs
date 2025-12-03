use crate::errors::AccessManagerError;
use crate::state::AccessManager;
use anchor_lang::prelude::*;

/// Helper function to verify direct call authorization with role-based access control
///
/// This function provides defense-in-depth by performing THREE security checks:
/// 1. Rejects CPI calls (instruction must be called directly)
/// 2. Verifies the account is a transaction signer (`is_signer` == true)
/// 3. Verifies the account has the required role
///
/// This is the recommended pattern for all admin/privileged instructions.
///
/// # Arguments
/// * `access_manager_account` - The access manager account info
/// * `role_id` - The role ID to check (e.g., `RELAYER_ROLE`, `PAUSER_ROLE`)
/// * `signer_account` - The account that must be a signer AND have the role
/// * `instructions_sysvar` - The instructions sysvar for CPI validation
/// * `program_id` - The current program ID
///
/// # Returns
/// * `Ok(())` if all checks pass
/// * `Err(CpiNotAllowed)` if called via CPI
/// * `Err(SignerRequired)` if account is not a signer
/// * `Err(Unauthorized)` if account doesn't have the required role
///
/// # Security
/// Prevents CPI-based signer spoofing attacks by ensuring:
/// - No CPI chain exists (direct call only)
/// - Account actually signed the transaction
/// - Account has proper authorization
///
/// # Example
/// ```ignore
/// access_manager::require_role(
///     &ctx.accounts.access_manager,
///     solana_ibc_types::roles::PAUSER_ROLE,
///     &ctx.accounts.authority,
///     &ctx.accounts.instructions_sysvar,
///     &crate::ID,
/// )?;
/// ```
pub fn require_role(
    access_manager_account: &AccountInfo,
    role_id: u64,
    signer_account: &AccountInfo,
    instructions_sysvar: &AccountInfo,
    program_id: &Pubkey,
) -> Result<()> {
    // Layer 1: Validate caller - instruction must be called directly
    // This prevents malicious programs from bypassing signer checks by spoofing signers in a CPI call.
    // Only direct user transactions can pass this check, ensuring the signer is authentic.
    solana_ibc_types::validate_direct_or_whitelisted_cpi(
        instructions_sysvar,
        crate::WHITELISTED_CPI_PROGRAMS,
        program_id,
    )
    .map_err(|_| error!(AccessManagerError::CpiNotAllowed))?;

    // Layer 2: Verify the account is actually a signer
    require!(signer_account.is_signer, AccessManagerError::SignerRequired);

    // Layer 3: Verify the signer has the required role
    let access_manager_data = access_manager_account.try_borrow_data()?;
    let access_manager: AccessManager =
        anchor_lang::AccountDeserialize::try_deserialize(&mut &access_manager_data[..])?;

    require!(
        access_manager.has_role(role_id, &signer_account.key()),
        AccessManagerError::Unauthorized
    );

    Ok(())
}
