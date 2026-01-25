use anchor_lang::prelude::*;

/// Custom errors for ICS27 IFT program
#[error_code]
pub enum IFTError {
    #[msg("Client ID cannot be empty")]
    EmptyClientId = 7000,

    #[msg("Counterparty address cannot be empty")]
    EmptyCounterpartyAddress,

    #[msg("Receiver cannot be empty")]
    EmptyReceiver,

    #[msg("Transfer amount must be greater than zero")]
    ZeroAmount,

    #[msg("IFT bridge not found for client")]
    BridgeNotFound,

    #[msg("IFT bridge is not active")]
    BridgeNotActive,

    #[msg("Invalid receiver address")]
    InvalidReceiver,

    #[msg("Timeout must be in the future")]
    TimeoutInPast,

    #[msg("Timeout too far in the future")]
    TimeoutTooLong,

    #[msg("Pending transfer not found")]
    PendingTransferNotFound,

    #[msg("Direct calls not allowed, must be called via CPI")]
    DirectCallNotAllowed,

    #[msg("Token account owner mismatch")]
    TokenAccountOwnerMismatch,

    #[msg("Invalid GMP program")]
    InvalidGmpProgram,

    #[msg("Invalid mint authority")]
    InvalidMintAuthority,

    #[msg("GMP call failed")]
    GmpCallFailed,

    #[msg("Invalid client ID length")]
    InvalidClientIdLength,

    #[msg("Invalid counterparty address length")]
    InvalidCounterpartyAddressLength,

    #[msg("Invalid sysvar account provided")]
    InvalidSysvar,

    #[msg("Unauthorized GMP caller")]
    UnauthorizedGmp,

    #[msg("Invalid GMP account - not derived from expected counterparty bridge")]
    InvalidGmpAccount,

    #[msg("Invalid pending transfer account")]
    InvalidPendingTransfer,

    #[msg("Mint decimals mismatch")]
    DecimalsMismatch,

    #[msg("GMP result client ID mismatch")]
    GmpResultClientMismatch,

    #[msg("GMP result sequence mismatch")]
    GmpResultSequenceMismatch,

    #[msg("GMP result sender mismatch")]
    GmpResultSenderMismatch,
}

/// Convert CPI validation errors to IFT errors
impl From<solana_ibc_types::CpiValidationError> for IFTError {
    fn from(err: solana_ibc_types::CpiValidationError) -> Self {
        match err {
            solana_ibc_types::CpiValidationError::InvalidSysvar => Self::InvalidSysvar,
            solana_ibc_types::CpiValidationError::DirectCallNotAllowed => {
                Self::DirectCallNotAllowed
            }
            solana_ibc_types::CpiValidationError::UnauthorizedCaller => Self::UnauthorizedGmp,
        }
    }
}
