use crate::errors::AccessManagerError;
use crate::events::{
    AccessManagerTransferAccepted, AccessManagerTransferCancelled, AccessManagerTransferProposed,
};
use crate::state::{AccessManager, AccessManagerTransferState};
use anchor_lang::prelude::*;

/// Verifies the caller has the `ADMIN_ROLE`.
///
/// Allows direct calls and whitelisted CPI callers (e.g. multisig) so admin
/// operations can go through governance. Reads the whitelist from the
/// `AccessManager` account state.
pub fn require_admin(
    access_manager_account: &AccountInfo,
    signer_account: &AccountInfo,
    instructions_sysvar: &AccountInfo,
    program_id: &Pubkey,
) -> Result<()> {
    let access_manager = deserialize_access_manager(access_manager_account)?;

    require_role_with_whitelist_inner(
        &access_manager,
        solana_ibc_types::roles::ADMIN_ROLE,
        signer_account,
        instructions_sysvar,
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
    require!(signer_account.is_signer, AccessManagerError::SignerRequired);

    solana_ibc_types::reject_cpi(instructions_sysvar, program_id)
        .map_err(|_| error!(AccessManagerError::CpiNotAllowed))?;

    let access_manager = deserialize_access_manager(access_manager_account)?;

    require!(
        access_manager.has_role(role_id, &signer_account.key()),
        AccessManagerError::Unauthorized
    );

    Ok(())
}

/// Verifies the caller has the given role. Allows direct calls and whitelisted
/// CPI callers. Reads the whitelist from the `AccessManager` account state.
pub fn require_role_with_whitelist(
    access_manager_account: &AccountInfo,
    role_id: u64,
    signer_account: &AccountInfo,
    instructions_sysvar: &AccountInfo,
    program_id: &Pubkey,
) -> Result<()> {
    let access_manager = deserialize_access_manager(access_manager_account)?;

    require_role_with_whitelist_inner(
        &access_manager,
        role_id,
        signer_account,
        instructions_sysvar,
        program_id,
    )
}

fn deserialize_access_manager(account: &AccountInfo) -> Result<AccessManager> {
    let data = account.try_borrow_data()?;
    anchor_lang::AccountDeserialize::try_deserialize(&mut &data[..])
}

fn require_role_with_whitelist_inner(
    access_manager: &AccessManager,
    role_id: u64,
    signer_account: &AccountInfo,
    instructions_sysvar: &AccountInfo,
    program_id: &Pubkey,
) -> Result<()> {
    require!(signer_account.is_signer, AccessManagerError::SignerRequired);

    solana_ibc_types::require_direct_call_or_whitelisted_caller(
        instructions_sysvar,
        &access_manager.whitelisted_programs,
        program_id,
    )
    .map_err(|_| error!(AccessManagerError::CpiNotAllowed))?;

    require!(
        access_manager.has_role(role_id, &signer_account.key()),
        AccessManagerError::Unauthorized
    );

    Ok(())
}

/// Proposes transferring the access manager to a new program.
///
/// Validates admin authorization against the current AM, rejects zero addresses
/// and self-transfers, and ensures no pending transfer already exists.
pub fn handle_propose_access_manager_transfer(
    state: &mut AccessManagerTransferState,
    new_access_manager: Pubkey,
    access_manager_account: &AccountInfo,
    admin: &AccountInfo,
    instructions_sysvar: &AccountInfo,
    program_id: &Pubkey,
) -> Result<()> {
    require_admin(
        access_manager_account,
        admin,
        instructions_sysvar,
        program_id,
    )?;

    require!(
        new_access_manager != Pubkey::default(),
        AccessManagerError::InvalidProposedAccessManager
    );

    require!(
        new_access_manager != state.access_manager,
        AccessManagerError::AccessManagerSelfTransfer
    );

    require!(
        state.pending_access_manager.is_none(),
        AccessManagerError::PendingAccessManagerTransferAlreadyExists
    );

    let current = state.access_manager;
    state.pending_access_manager = Some(new_access_manager);

    emit!(AccessManagerTransferProposed {
        current_access_manager: current,
        proposed_access_manager: new_access_manager,
    });

    Ok(())
}

/// Accepts a pending access manager transfer.
///
/// Validates that there is a pending transfer, derives the expected PDA from
/// the pending program ID, verifies the provided account matches, and checks
/// admin authorization against the **new** AM to prove it is valid.
pub fn handle_accept_access_manager_transfer(
    state: &mut AccessManagerTransferState,
    new_access_manager_account: &AccountInfo,
    admin: &AccountInfo,
    instructions_sysvar: &AccountInfo,
    program_id: &Pubkey,
) -> Result<()> {
    let pending_am_program = state
        .pending_access_manager
        .ok_or(error!(AccessManagerError::NoPendingAccessManagerTransfer))?;

    let (expected_pda, _) =
        Pubkey::find_program_address(&[AccessManager::SEED], &pending_am_program);

    require!(
        new_access_manager_account.key() == expected_pda,
        AccessManagerError::InvalidProposedAccessManager
    );

    require_admin(
        new_access_manager_account,
        admin,
        instructions_sysvar,
        program_id,
    )?;

    let old = state.access_manager;
    state.access_manager = pending_am_program;
    state.pending_access_manager = None;

    emit!(AccessManagerTransferAccepted {
        old_access_manager: old,
        new_access_manager: pending_am_program,
    });

    Ok(())
}

/// Cancels a pending access manager transfer.
///
/// Validates admin authorization against the current AM and clears the
/// pending transfer.
pub fn handle_cancel_access_manager_transfer(
    state: &mut AccessManagerTransferState,
    access_manager_account: &AccountInfo,
    admin: &AccountInfo,
    instructions_sysvar: &AccountInfo,
    program_id: &Pubkey,
) -> Result<()> {
    require_admin(
        access_manager_account,
        admin,
        instructions_sysvar,
        program_id,
    )?;

    let pending = state
        .pending_access_manager
        .ok_or(error!(AccessManagerError::NoPendingAccessManagerTransfer))?;

    let current = state.access_manager;
    state.pending_access_manager = None;

    emit!(AccessManagerTransferCancelled {
        access_manager: current,
        cancelled_access_manager: pending,
    });

    Ok(())
}
