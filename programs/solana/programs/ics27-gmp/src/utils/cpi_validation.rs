use crate::errors::GMPError;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::sysvar::instructions::get_instruction_relative;

/// Validates that this instruction is called via CPI from the authorized program
///
/// Checks:
/// 1. `instruction_sysvar` is the real sysvar (prevents [Wormhole-style attack])
/// 2. Current instruction's `program_id` is the authorized program (rejects direct calls)
///
/// [Wormhole-style attack]: https://github.com/sbellem/wormhole-attack-analysis
/// [Metaplex pattern]: https://github.com/metaplex-foundation/metaplex-program-library/blob/27bd23f5884d4f0d64ae8f3c7bafeeaffc53e620/candy-machine/program/src/processor/mint.rs#L115
pub fn validate_cpi_caller(
    instruction_sysvar: &AccountInfo<'_>,
    authorized_program: &Pubkey,
) -> Result<()> {
    // CRITICAL: Validate that the instruction_sysvar account is actually the instructions sysvar
    // This prevents attacks where a malicious actor passes a fake sysvar account
    // See Wormhole attack: https://github.com/sbellem/wormhole-attack-analysis
    require!(
        instruction_sysvar.key() == anchor_lang::solana_program::sysvar::instructions::ID,
        GMPError::InvalidSysvar
    );

    // Get the current instruction (0 = current, relative offset)
    let current_ix = get_instruction_relative(0, instruction_sysvar)?;

    // Reject direct calls (when current instruction is our own program)
    require!(
        current_ix.program_id != crate::ID,
        GMPError::DirectCallNotAllowed
    );

    // Verify the calling program is the authorized router
    require!(
        current_ix.program_id == *authorized_program,
        GMPError::UnauthorizedRouter
    );

    Ok(())
}
