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

    #[msg("IFT bridge mint does not match app state mint")]
    InvalidBridge,

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

    #[msg("GMP call failed")]
    GmpCallFailed,

    #[msg("Invalid client ID length")]
    InvalidClientIdLength,

    #[msg("Invalid counterparty address length")]
    InvalidCounterpartyAddressLength,

    #[msg("Counterparty denom cannot be empty for Cosmos chains")]
    CosmosEmptyCounterpartyDenom,

    #[msg("Invalid counterparty denom length")]
    InvalidCounterpartyDenomLength,

    #[msg("Cosmos type URL cannot be empty for Cosmos chains")]
    CosmosEmptyTypeUrl,

    #[msg("Invalid Cosmos type URL length")]
    InvalidCosmosTypeUrlLength,

    #[msg("Cosmos ICA address cannot be empty for Cosmos chains")]
    CosmosEmptyIcaAddress,

    #[msg("Invalid Cosmos ICA address length")]
    InvalidCosmosIcaAddressLength,

    #[msg("Invalid sysvar account provided")]
    InvalidSysvar,

    #[msg("Unauthorized GMP caller")]
    UnauthorizedGmp,

    #[msg("Invalid GMP account - not derived from expected counterparty bridge")]
    InvalidGmpAccount,

    #[msg("Invalid pending transfer account")]
    InvalidPendingTransfer,

    #[msg("GMP result client ID mismatch")]
    GmpResultClientMismatch,

    #[msg("GMP result sequence mismatch")]
    GmpResultSequenceMismatch,

    #[msg("GMP result sender mismatch")]
    GmpResultSenderMismatch,

    #[msg("Mint authority is not set on the token")]
    MintAuthorityNotSet,

    #[msg("Invalid mint authority - signer does not match current authority")]
    InvalidMintAuthority,

    #[msg("Daily mint rate limit exceeded")]
    MintRateLimitExceeded,
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
