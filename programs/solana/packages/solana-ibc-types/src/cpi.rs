use anchor_lang::prelude::*;
use anchor_lang::solana_program::sysvar::instructions::get_instruction_relative;

// # How `get_instruction_relative(0, ...)` works
//
// The Instructions sysvar stores only top-level transaction instructions,
// not nested CPI calls. During CPI, the instruction index doesn't change.
//
// ## Direct call - instruction IS our program:
//
//   Transaction
//   ├─ Instruction 0: Program B (we are here, direct call)
//   │                 get_instruction_relative(0) → Instruction 0
//   │                 current_ix.program_id == self_program_id ✓
//   │
//   └─ Instruction 1: Program C
//
// ## CPI call - instruction is the CALLER's program:
//
//   Transaction
//   ├─ Instruction 0: Program A ──┐
//   │                             │ CPI
//   │                             ▼
//   │                      Program B (we are here)
//   │                      get_instruction_relative(0) → Instruction 0
//   │                      current_ix.program_id == Program A (caller)
//   │
//   └─ Instruction 1: Program C
//
// This allows us to verify WHO initiated the call.

#[error_code]
pub enum CpiValidationError {
    #[msg("Invalid sysvar account")]
    InvalidSysvar,
    #[msg("Direct call not allowed - must be called via CPI")]
    DirectCallNotAllowed,
    #[msg("Unauthorized CPI caller")]
    UnauthorizedCaller,
    #[msg("External CPI not allowed - instruction must originate from this program")]
    ExternalCpiNotAllowed,
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

    // Get the current instruction (0 = current, relative offset) - see above explanation
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

/// Validates that this program is the entrypoint of the current transaction instruction
///
/// Checks:
/// 1. `instruction_sysvar` is the real sysvar (prevents [Wormhole-style attack])
/// 2. Top-level instruction's `program_id` IS self (rejects external CPI callers)
///
/// **Important**: This does NOT prevent recursive CPI (A → B → A). It only ensures
/// our program is the top-level instruction. For most admin use cases this is sufficient
/// since it prevents external programs from triggering admin operations via CPI.
///
/// Use this for admin instructions that should only be initiated by direct user transactions
/// or by our own internal CPI calls.
pub fn reject_cpi(
    instruction_sysvar: &AccountInfo<'_>,
    self_program_id: &Pubkey,
) -> core::result::Result<(), CpiValidationError> {
    // CRITICAL: Validate that the instruction_sysvar account is actually the instructions sysvar
    if instruction_sysvar.key() != anchor_lang::solana_program::sysvar::instructions::ID {
        return Err(CpiValidationError::InvalidSysvar);
    }

    // Get the current instruction (0 = current, relative offset) - see above explanation
    let current_ix = get_instruction_relative(0, instruction_sysvar)
        .map_err(|_| CpiValidationError::InvalidSysvar)?;

    // Reject external CPI (when top-level instruction is NOT our own program)
    if current_ix.program_id != *self_program_id {
        return Err(CpiValidationError::ExternalCpiNotAllowed);
    }

    Ok(())
}
