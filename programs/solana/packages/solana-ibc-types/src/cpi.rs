use anchor_lang::{
    error_code,
    prelude::{AccountInfo, Pubkey},
    solana_program::sysvar::instructions::get_instruction_relative,
    Key,
};

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

/// Validates that this instruction is either called directly OR via CPI from a whitelisted program
///
/// Checks:
/// 1. `instruction_sysvar` is the real sysvar (prevents [Wormhole-style attack])
/// 2. Current instruction's `program_id` is either self (direct call) or in the whitelist
///
/// Use this for instructions that can be called both directly by users and via CPI from trusted programs.
pub fn require_direct_call_or_whitelisted_caller(
    instruction_sysvar: &AccountInfo<'_>,
    whitelisted_programs: &[Pubkey],
    self_program_id: &Pubkey,
) -> core::result::Result<(), CpiValidationError> {
    // CRITICAL: Validate that the instruction_sysvar account is actually the instructions sysvar
    if instruction_sysvar.key() != anchor_lang::solana_program::sysvar::instructions::ID {
        return Err(CpiValidationError::InvalidSysvar);
    }

    // Get the current instruction (0 = current, relative offset) - see above explanation
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

/// Validates that this program is the entrypoint of the current transaction instruction
///
/// Checks:
/// 1. `instruction_sysvar` is the real sysvar (prevents [Wormhole-style attack])
/// 2. Top-level instruction's `program_id` IS self (rejects external CPI callers)
///
/// **Important**: This does NOT prevent recursive CPI (A → B → A). It only ensures
/// our program is the top-level instruction.
pub fn reject_cpi(
    instruction_sysvar: &AccountInfo<'_>,
    self_program_id: &Pubkey,
) -> core::result::Result<(), CpiValidationError> {
    require_direct_call_or_whitelisted_caller(instruction_sysvar, &[], self_program_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use anchor_lang::solana_program::sysvar::instructions::ID as INSTRUCTIONS_SYSVAR_ID;
    use solana_sdk::sysvar::instructions::{
        construct_instructions_data, BorrowedAccountMeta, BorrowedInstruction,
    };

    fn create_instructions_sysvar_data(caller_program_id: &Pubkey) -> Vec<u8> {
        let account_pubkey = Pubkey::new_unique();
        let account = BorrowedAccountMeta {
            pubkey: &account_pubkey,
            is_signer: false,
            is_writable: true,
        };
        let instruction = BorrowedInstruction {
            program_id: caller_program_id,
            accounts: vec![account],
            data: &[],
        };
        construct_instructions_data(&[instruction])
    }

    fn create_test_account_info<'a>(
        key: &'a Pubkey,
        lamports: &'a mut u64,
        data: &'a mut [u8],
        owner: &'a Pubkey,
    ) -> AccountInfo<'a> {
        AccountInfo::new(key, false, false, lamports, data, owner, false, 0)
    }

    #[test]
    fn test_validate_cpi_caller_authorized_succeeds() {
        let authorized_program = Pubkey::new_unique();
        let self_program_id = Pubkey::new_unique();

        let mut data = create_instructions_sysvar_data(&authorized_program);
        let mut lamports = 1_000_000u64;
        let sysvar_owner = anchor_lang::solana_program::sysvar::ID;

        let account_info = create_test_account_info(
            &INSTRUCTIONS_SYSVAR_ID,
            &mut lamports,
            &mut data,
            &sysvar_owner,
        );

        let result = validate_cpi_caller(&account_info, &authorized_program, &self_program_id);

        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_cpi_caller_unauthorized_fails() {
        let authorized_program = Pubkey::new_unique();
        let unauthorized_caller = Pubkey::new_unique();
        let self_program_id = Pubkey::new_unique();

        let mut data = create_instructions_sysvar_data(&unauthorized_caller);
        let mut lamports = 1_000_000u64;
        let sysvar_owner = anchor_lang::solana_program::sysvar::ID;

        let account_info = create_test_account_info(
            &INSTRUCTIONS_SYSVAR_ID,
            &mut lamports,
            &mut data,
            &sysvar_owner,
        );

        let result = validate_cpi_caller(&account_info, &authorized_program, &self_program_id);

        assert!(matches!(
            result,
            Err(CpiValidationError::UnauthorizedCaller)
        ));
    }

    #[test]
    fn test_validate_cpi_caller_direct_call_fails() {
        let authorized_program = Pubkey::new_unique();
        let self_program_id = Pubkey::new_unique();

        let mut data = create_instructions_sysvar_data(&self_program_id);
        let mut lamports = 1_000_000u64;
        let sysvar_owner = anchor_lang::solana_program::sysvar::ID;

        let account_info = create_test_account_info(
            &INSTRUCTIONS_SYSVAR_ID,
            &mut lamports,
            &mut data,
            &sysvar_owner,
        );

        let result = validate_cpi_caller(&account_info, &authorized_program, &self_program_id);

        assert!(matches!(
            result,
            Err(CpiValidationError::DirectCallNotAllowed)
        ));
    }

    #[test]
    fn test_validate_direct_or_whitelisted_direct_call_succeeds() {
        let self_program_id = Pubkey::new_unique();
        let whitelisted: Vec<Pubkey> = vec![];

        let mut data = create_instructions_sysvar_data(&self_program_id);
        let mut lamports = 1_000_000u64;
        let sysvar_owner = anchor_lang::solana_program::sysvar::ID;

        let account_info = create_test_account_info(
            &INSTRUCTIONS_SYSVAR_ID,
            &mut lamports,
            &mut data,
            &sysvar_owner,
        );

        let result =
            validate_direct_or_whitelisted_cpi(&account_info, &whitelisted, &self_program_id);

        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_direct_or_whitelisted_cpi_from_whitelist_succeeds() {
        let self_program_id = Pubkey::new_unique();
        let whitelisted_caller = Pubkey::new_unique();
        let whitelisted = vec![whitelisted_caller];

        let mut data = create_instructions_sysvar_data(&whitelisted_caller);
        let mut lamports = 1_000_000u64;
        let sysvar_owner = anchor_lang::solana_program::sysvar::ID;

        let account_info = create_test_account_info(
            &INSTRUCTIONS_SYSVAR_ID,
            &mut lamports,
            &mut data,
            &sysvar_owner,
        );

        let result =
            validate_direct_or_whitelisted_cpi(&account_info, &whitelisted, &self_program_id);

        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_direct_or_whitelisted_unauthorized_fails() {
        let self_program_id = Pubkey::new_unique();
        let unauthorized_caller = Pubkey::new_unique();
        let whitelisted: Vec<Pubkey> = vec![];

        let mut data = create_instructions_sysvar_data(&unauthorized_caller);
        let mut lamports = 1_000_000u64;
        let sysvar_owner = anchor_lang::solana_program::sysvar::ID;

        let account_info = create_test_account_info(
            &INSTRUCTIONS_SYSVAR_ID,
            &mut lamports,
            &mut data,
            &sysvar_owner,
        );

        let result =
            validate_direct_or_whitelisted_cpi(&account_info, &whitelisted, &self_program_id);

        assert!(matches!(
            result,
            Err(CpiValidationError::UnauthorizedCaller)
        ));
    }

    #[test]
    fn test_reject_cpi_direct_call_succeeds() {
        let self_program_id = Pubkey::new_unique();

        let mut data = create_instructions_sysvar_data(&self_program_id);
        let mut lamports = 1_000_000u64;
        let sysvar_owner = anchor_lang::solana_program::sysvar::ID;

        let account_info = create_test_account_info(
            &INSTRUCTIONS_SYSVAR_ID,
            &mut lamports,
            &mut data,
            &sysvar_owner,
        );

        let result = reject_cpi(&account_info, &self_program_id);

        assert!(result.is_ok());
    }

    #[test]
    fn test_reject_cpi_cpi_call_fails() {
        let self_program_id = Pubkey::new_unique();
        let cpi_caller = Pubkey::new_unique();

        let mut data = create_instructions_sysvar_data(&cpi_caller);
        let mut lamports = 1_000_000u64;
        let sysvar_owner = anchor_lang::solana_program::sysvar::ID;

        let account_info = create_test_account_info(
            &INSTRUCTIONS_SYSVAR_ID,
            &mut lamports,
            &mut data,
            &sysvar_owner,
        );

        let result = reject_cpi(&account_info, &self_program_id);

        assert!(matches!(
            result,
            Err(CpiValidationError::UnauthorizedCaller)
        ));
    }
}
