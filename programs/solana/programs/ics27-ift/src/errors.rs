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

    #[msg("Unauthorized mint caller")]
    UnauthorizedMint,

    #[msg("Invalid receiver address")]
    InvalidReceiver,

    #[msg("Timeout must be in the future")]
    TimeoutInPast,

    #[msg("Timeout too far in the future")]
    TimeoutTooLong,

    #[msg("Pending transfer not found")]
    PendingTransferNotFound,

    #[msg("Refund account missing")]
    RefundAccountMissing,

    #[msg("Failed to parse sequence from router")]
    SequenceParseError,

    #[msg("Direct calls not allowed, must be called via CPI")]
    DirectCallNotAllowed,

    #[msg("Unauthorized router calling")]
    UnauthorizedRouter,

    #[msg("Invalid acknowledgement format")]
    InvalidAcknowledgement,

    #[msg("Unauthorized admin operation")]
    UnauthorizedAdmin,

    #[msg("Salt must be empty for IFT")]
    SaltNotEmpty,

    #[msg("Token account owner mismatch")]
    TokenAccountOwnerMismatch,

    #[msg("Invalid GMP program")]
    InvalidGmpProgram,

    #[msg("Bridge already exists")]
    BridgeAlreadyExists,

    #[msg("Invalid mint authority")]
    InvalidMintAuthority,

    #[msg("Token mint failed")]
    TokenMintFailed,

    #[msg("Token burn failed")]
    TokenBurnFailed,

    #[msg("Invalid counterparty chain type")]
    InvalidChainType,

    #[msg("GMP call failed")]
    GmpCallFailed,

    #[msg("Invalid client ID length")]
    InvalidClientIdLength,

    #[msg("Invalid counterparty address length")]
    InvalidCounterpartyAddressLength,

    #[msg("Pending transfer already exists")]
    PendingTransferExists,

    #[msg("Invalid sysvar account provided")]
    InvalidSysvar,

    #[msg("Unauthorized GMP caller")]
    UnauthorizedGmp,

    #[msg("Invalid GMP account - not derived from expected counterparty bridge")]
    InvalidGmpAccount,
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
