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
    #[msg("Packet commitment already exists")]
    PacketCommitmentAlreadyExists,
    #[msg("Packet commitment mismatch")]
    PacketCommitmentMismatch,
    #[msg("Packet should have payload")]
    PacketNoPayload,
    #[msg("Packet receipt mismatch")]
    PacketReceiptMismatch,
    #[msg("Packet acknowledgement already exists")]
    PacketAcknowledgementAlreadyExists,
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
    #[msg("Exceeds maximum batch size")]
    ExceedsMaxBatchSize,
    #[msg("Empty cleanup batch")]
    EmptyCleanupBatch,
    #[msg("Missing account in remaining accounts")]
    MissingAccount,
    #[msg("Invalid account")]
    InvalidAccount,
    #[msg("Unsupported account version")]
    UnsupportedVersion,
}
