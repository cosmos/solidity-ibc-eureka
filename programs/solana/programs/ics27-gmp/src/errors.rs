use anchor_lang::prelude::*;

/// Custom errors for ICS27 GMP program
#[error_code]
pub enum GMPError {
    #[msg("App is currently paused")]
    AppPaused = 6000,

    #[msg("Invalid router program")]
    InvalidRouter,

    #[msg("Execution payload is empty")]
    EmptyPayload,

    #[msg("Invalid timeout timestamp")]
    InvalidTimeout,

    #[msg("Timeout too far in future")]
    TimeoutTooLong,

    #[msg("Timeout too soon")]
    TimeoutTooSoon,

    #[msg("Unauthorized sender")]
    UnauthorizedSender,

    #[msg("Wrong counterparty client")]
    WrongCounterpartyClient,

    #[msg("Invalid salt")]
    InvalidSalt,

    #[msg("Target program is not executable")]
    TargetNotExecutable,

    #[msg("Insufficient accounts provided")]
    InsufficientAccounts,

    #[msg("Account key mismatch")]
    AccountKeyMismatch,

    #[msg("Insufficient account permissions")]
    InsufficientAccountPermissions,

    #[msg("Unauthorized signer")]
    UnauthorizedSigner,

    #[msg("Execution too expensive")]
    ExecutionTooExpensive,

    #[msg("Invalid account address derivation")]
    InvalidAccountAddress,

    #[msg("Unauthorized admin operation")]
    UnauthorizedAdmin,

    #[msg("Invalid packet data format")]
    InvalidPacketData,

    #[msg("Unauthorized router calling")]
    UnauthorizedRouter,

    #[msg("Port ID too long")]
    PortIdTooLong,

    #[msg("Client ID too long")]
    ClientIdTooLong,

    #[msg("Sender address too long")]
    SenderTooLong,

    #[msg("Salt too long")]
    SaltTooLong,

    #[msg("Memo too long")]
    MemoTooLong,

    #[msg("Payload too long")]
    PayloadTooLong,

    #[msg("Invalid execution payload format")]
    InvalidExecutionPayload,

    #[msg("Account already exists")]
    AccountAlreadyExists,

    #[msg("Failed to parse packet data")]
    PacketDataParseError,

    #[msg("Packet data validation failed")]
    PacketDataValidationFailed,

    #[msg("Target program execution failed")]
    TargetExecutionFailed,

    #[msg("Invalid acknowledgement format")]
    InvalidAcknowledgement,

    #[msg("Compute budget exceeded")]
    ComputeBudgetExceeded,

    #[msg("Too many accounts in execution payload")]
    TooManyAccounts,

    #[msg("Invalid program ID format")]
    InvalidProgramId,

    #[msg("Invalid account key format")]
    InvalidAccountKey,

    #[msg("Invalid router CPI call")]
    InvalidRouterCall,

    #[msg("Insufficient funds for account creation")]
    InsufficientFunds,

    #[msg("Invalid payer position")]
    InvalidPayerPosition,

    #[msg("Invalid IBC version")]
    InvalidVersion,

    #[msg("Invalid IBC port")]
    InvalidPort,

    #[msg("Invalid IBC encoding")]
    InvalidEncoding,

    #[msg("Failed to parse sequence from router account")]
    SequenceParseError,
}
