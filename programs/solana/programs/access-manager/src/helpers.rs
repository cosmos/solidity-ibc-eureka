use crate::errors::AccessManagerError;
use crate::state::AccessManager;
use anchor_lang::prelude::*;

/// Verifies the caller has the `ADMIN_ROLE`. Allows direct calls and whitelisted
/// CPI callers (e.g. multisig) so admin operations can go through governance.
pub fn require_admin(
    access_manager_account: &AccountInfo,
    signer_account: &AccountInfo,
    instructions_sysvar: &AccountInfo,
    program_id: &Pubkey,
) -> Result<()> {
    require_role_with_whitelist(
        access_manager_account,
        solana_ibc_types::roles::ADMIN_ROLE,
        signer_account,
        instructions_sysvar,
        crate::WHITELISTED_CPI_PROGRAMS,
        program_id,
    )
}

/// Verifies the caller has the given role. Rejects all CPI calls — only direct
/// transactions are allowed.
pub fn require_role(
    access_manager_account: &AccountInfo,
    role_id: u64,
    signer_account: &AccountInfo,
    instructions_sysvar: &AccountInfo,
    program_id: &Pubkey,
) -> Result<()> {
    solana_ibc_types::reject_cpi(instructions_sysvar, program_id)
        .map_err(|_| error!(AccessManagerError::CpiNotAllowed))?;

    verify_signer_has_role(access_manager_account, role_id, signer_account)
}

/// Verifies the caller has the given role. Allows direct calls and whitelisted
/// CPI callers.
pub fn require_role_with_whitelist(
    access_manager_account: &AccountInfo,
    role_id: u64,
    signer_account: &AccountInfo,
    instructions_sysvar: &AccountInfo,
    whitelisted_programs: &[Pubkey],
    program_id: &Pubkey,
) -> Result<()> {
    solana_ibc_types::require_direct_call_or_whitelisted_caller(
        instructions_sysvar,
        whitelisted_programs,
        program_id,
    )
    .map_err(|_| error!(AccessManagerError::CpiNotAllowed))?;

    verify_signer_has_role(access_manager_account, role_id, signer_account)
}

fn verify_signer_has_role(
    access_manager_account: &AccountInfo,
    role_id: u64,
    signer_account: &AccountInfo,
) -> Result<()> {
    require!(signer_account.is_signer, AccessManagerError::SignerRequired);

    let access_manager_data = access_manager_account.try_borrow_data()?;
    let access_manager: AccessManager =
        anchor_lang::AccountDeserialize::try_deserialize(&mut &access_manager_data[..])?;

    require!(
        access_manager.has_role(role_id, &signer_account.key()),
        AccessManagerError::Unauthorized
    );

    Ok(())
}
