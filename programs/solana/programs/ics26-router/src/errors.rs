use anchor_lang::prelude::*;

#[error_code]
pub enum RouterError {
    #[msg("Unauthorized sender")]
    UnauthorizedSender,
    #[msg("Port already bound to IBC app")]
    PortAlreadyBound,
    #[msg("Port not found")]
    PortNotFound,
    #[msg("Invalid port identifier")]
    InvalidPortIdentifier,
    #[msg("Invalid timeout timestamp")]
    InvalidTimeoutTimestamp,
    #[msg("Invalid timeout duration")]
    InvalidTimeoutDuration,
    #[msg("Invalid counterparty")]
    InvalidCounterparty,
    #[msg("Packet commitment mismatch")]
    PacketCommitmentMismatch,
    #[msg("Packet receipt mismatch")]
    PacketReceiptMismatch,
    #[msg("Multi-payload packets not supported")]
    MultiPayloadPacketNotSupported,
    #[msg("Async acknowledgement not supported")]
    AsyncAcknowledgementNotSupported,
    #[msg("Universal error acknowledgement")]
    UniversalErrorAcknowledgement,
    #[msg("Failed callback")]
    FailedCallback,
    #[msg("No acknowledgements")]
    NoAcknowledgements,
    #[msg("Invalid merkle prefix")]
    InvalidMerklePrefix,
    #[msg("Client not found")]
    ClientNotFound,
    #[msg("Unauthorized authority")]
    UnauthorizedAuthority,
    #[msg("Invalid client ID")]
    InvalidClientId,
    #[msg("Invalid light client program")]
    InvalidLightClientProgram,
    #[msg("Unsupported client type")]
    UnsupportedClientType,
    #[msg("Invalid counterparty info")]
    InvalidCounterpartyInfo,
    #[msg("Client already exists")]
    ClientAlreadyExists,
    #[msg("Client not active")]
    ClientNotActive,
    #[msg("Invalid counterparty client")]
    InvalidCounterpartyClient,
    #[msg("Wrong client passed")]
    ClientMismatch,
    #[msg("Arithmetic overflow")]
    ArithmeticOverflow,
    #[msg("Invalid response from IBC app")]
    InvalidAppResponse,
    #[msg("IBC app not found")]
    IbcAppNotFound,
    #[msg("Chunk data too large")]
    ChunkDataTooLarge,
    #[msg("Invalid chunk account")]
    InvalidChunkAccount,
    #[msg("Invalid chunk count")]
    InvalidChunkCount,
    #[msg("Invalid chunk commitment")]
    InvalidChunkCommitment,
    #[msg("Invalid payload count")]
    InvalidPayloadCount,
    #[msg("Unsupported account version")]
    UnsupportedVersion,
    #[msg("Invalid migration params: at least one field must be updated")]
    InvalidMigrationParams,

    #[msg("Invalid sysvar account provided")]
    InvalidSysvar,

    #[msg("Direct calls not allowed, must be called via CPI")]
    DirectCallNotAllowed,

    #[msg("Invalid account owner: account is not owned by the expected program")]
    InvalidAccountOwner,

    #[msg("Invalid sequence suffix: must match hash(program_id || sender) % 10000")]
    InvalidSequenceSuffix,
}

/// Convert CPI validation errors to Router errors
impl From<solana_ibc_types::CpiValidationError> for RouterError {
    fn from(err: solana_ibc_types::CpiValidationError) -> Self {
        match err {
            solana_ibc_types::CpiValidationError::InvalidSysvar => Self::InvalidSysvar,
            solana_ibc_types::CpiValidationError::DirectCallNotAllowed => {
                Self::DirectCallNotAllowed
            }
            solana_ibc_types::CpiValidationError::UnauthorizedCaller => Self::UnauthorizedSender,
        }
    }
}
