use anchor_lang::{
    error_code,
    prelude::{AccountInfo, Pubkey},
    solana_program::sysvar::instructions::get_instruction_relative,
    Key,
};

#[error_code]
pub enum CpiValidationError {
    #[msg("Invalid sysvar account")]
    InvalidSysvar,
    #[msg("Direct call not allowed - must be called via CPI")]
    DirectCallNotAllowed,
    #[msg("Unauthorized CPI caller")]
    UnauthorizedCaller,
}

/// Validates that this instruction is called via CPI from the authorized program
///
/// Checks:
/// 1. `instruction_sysvar` is the real sysvar (prevents [Wormhole-style attack])
/// 2. Current instruction's `program_id` is NOT self (rejects direct calls)
/// 3. Current instruction's `program_id` is the authorized program
///
/// [Wormhole-style attack]: https://github.com/sbellem/wormhole-attack-analysis
/// [Metaplex pattern]: https://github.com/metaplex-foundation/metaplex-program-library/blob/27bd23f5884d4f0d64ae8f3c7bafeeaffc53e620/candy-machine/program/src/processor/mint.rs#L115
pub fn validate_cpi_caller(
    instruction_sysvar: &AccountInfo<'_>,
    authorized_program: &Pubkey,
    self_program_id: &Pubkey,
) -> Result<(), CpiValidationError> {
    // CRITICAL: Validate that the instruction_sysvar account is actually the instructions sysvar
    // This prevents attacks where a malicious actor passes a fake sysvar account
    // See Wormhole attack: https://github.com/sbellem/wormhole-attack-analysis
    if instruction_sysvar.key() != anchor_lang::solana_program::sysvar::instructions::ID {
        return Err(CpiValidationError::InvalidSysvar);
    }

    // Get the current instruction (0 = current, relative offset)
    let current_ix = get_instruction_relative(0, instruction_sysvar)
        .map_err(|_| CpiValidationError::InvalidSysvar)?;

    // Reject direct calls (when current instruction is our own program)
    if current_ix.program_id == *self_program_id {
        return Err(CpiValidationError::DirectCallNotAllowed);
    }

    // Verify the calling program is the authorized program
    if current_ix.program_id != *authorized_program {
        return Err(CpiValidationError::UnauthorizedCaller);
    }

    Ok(())
}

/// Validates that this instruction is called via CPI from the authorized program OR an upstream caller
///
/// This extends `validate_cpi_caller` to support layered architectures (e.g., IFT → GMP → Router)
/// where the top-level program (IFT) differs from the registered app (GMP).
///
/// Checks:
/// 1. `instruction_sysvar` is the real sysvar (prevents [Wormhole-style attack])
/// 2. Current instruction's `program_id` is NOT self (rejects direct calls)
/// 3. Current instruction's `program_id` is either:
///    - The authorized program, OR
///    - One of the upstream callers
pub fn validate_cpi_caller_with_upstream(
    instruction_sysvar: &AccountInfo<'_>,
    authorized_program: &Pubkey,
    upstream_callers: &[Pubkey],
    self_program_id: &Pubkey,
) -> Result<(), CpiValidationError> {
    if instruction_sysvar.key() != anchor_lang::solana_program::sysvar::instructions::ID {
        return Err(CpiValidationError::InvalidSysvar);
    }

    let current_ix = get_instruction_relative(0, instruction_sysvar)
        .map_err(|_| CpiValidationError::InvalidSysvar)?;

    if current_ix.program_id == *self_program_id {
        return Err(CpiValidationError::DirectCallNotAllowed);
    }

    if current_ix.program_id == *authorized_program {
        return Ok(());
    }

    if upstream_callers.contains(&current_ix.program_id) {
        return Ok(());
    }

    Err(CpiValidationError::UnauthorizedCaller)
}

/// Validates that this instruction is either called directly OR via CPI from a whitelisted program
///
/// Checks:
/// 1. `instruction_sysvar` is the real sysvar (prevents [Wormhole-style attack])
/// 2. Current instruction's `program_id` is either self (direct call) or in the whitelist
///
/// Use this for instructions that can be called both directly by users and via CPI from trusted programs.
pub fn validate_direct_or_whitelisted_cpi(
    instruction_sysvar: &AccountInfo<'_>,
    whitelisted_programs: &[Pubkey],
    self_program_id: &Pubkey,
) -> core::result::Result<(), CpiValidationError> {
    // CRITICAL: Validate that the instruction_sysvar account is actually the instructions sysvar
    if instruction_sysvar.key() != anchor_lang::solana_program::sysvar::instructions::ID {
        return Err(CpiValidationError::InvalidSysvar);
    }

    // Get the current instruction (0 = current, relative offset)
    let current_ix = get_instruction_relative(0, instruction_sysvar)
        .map_err(|_| CpiValidationError::InvalidSysvar)?;

    // Allow direct calls
    if current_ix.program_id == *self_program_id {
        return Ok(());
    }

    // Allow CPI from whitelisted programs
    if whitelisted_programs.contains(&current_ix.program_id) {
        return Ok(());
    }

    Err(CpiValidationError::UnauthorizedCaller)
}

/// Validates that this instruction is called directly.
///
/// Checks:
/// 1. `instruction_sysvar` is the real sysvar (prevents [Wormhole-style attack])
/// 2. Current instruction's `program_id` IS self (rejects CPI calls)
pub fn reject_cpi(
    instruction_sysvar: &AccountInfo<'_>,
    self_program_id: &Pubkey,
) -> core::result::Result<(), CpiValidationError> {
    validate_direct_or_whitelisted_cpi(instruction_sysvar, &[], self_program_id)
}
