use anchor_lang::prelude::*;
use anchor_lang::solana_program::sysvar::instructions::get_instruction_relative;

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
) -> core::result::Result<(), CpiValidationError> {
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

/// Validates that this instruction is called directly (NOT via CPI)
///
/// Checks:
/// 1. `instruction_sysvar` is the real sysvar (prevents [Wormhole-style attack])
/// 2. Current instruction's `program_id` IS self (rejects CPI calls)
///
/// Use this for admin instructions that should only be called directly by users.
pub fn reject_cpi(
    instruction_sysvar: &AccountInfo<'_>,
    self_program_id: &Pubkey,
) -> core::result::Result<(), CpiValidationError> {
    // CRITICAL: Validate that the instruction_sysvar account is actually the instructions sysvar
    if instruction_sysvar.key() != anchor_lang::solana_program::sysvar::instructions::ID {
        return Err(CpiValidationError::InvalidSysvar);
    }

    // Get the current instruction (0 = current, relative offset)
    let current_ix = get_instruction_relative(0, instruction_sysvar)
        .map_err(|_| CpiValidationError::InvalidSysvar)?;

    // Reject CPI calls (when current instruction is NOT our own program)
    if current_ix.program_id != *self_program_id {
        return Err(CpiValidationError::UnauthorizedCaller);
    }

    Ok(())
}
