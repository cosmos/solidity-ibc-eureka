use anchor_lang::prelude::*;
use solana_ibc_types::CpiValidationError;

#[error_code]
pub enum AccessManagerError {
    #[msg("Unauthorized: caller does not have required role")]
    Unauthorized,
    #[msg("Invalid role ID")]
    InvalidRoleId,
    #[msg("Cannot remove the last admin")]
    CannotRemoveLastAdmin,
    #[msg("Account does not have the specified role")]
    RoleNotGranted,
    #[msg("Invalid sysenv: cross-program invocation from unexpected program")]
    InvalidSysenv,
    #[msg("Account must be a signer")]
    SignerRequired,
    #[msg("CPI calls not allowed")]
    CpiNotAllowed,
    #[msg("Program account does not match target_program")]
    ProgramMismatch,
}

impl From<CpiValidationError> for AccessManagerError {
    fn from(error: CpiValidationError) -> Self {
        match error {
            CpiValidationError::InvalidSysvar => Self::InvalidSysenv,
            CpiValidationError::UnauthorizedCaller
            | CpiValidationError::DirectCallNotAllowed
            | CpiValidationError::NestedCpiNotAllowed => Self::CpiNotAllowed,
        }
    }
}
