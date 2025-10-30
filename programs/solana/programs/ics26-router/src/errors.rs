use anchor_lang::prelude::*;

#[error_code]
pub enum RouterError {
    #[msg("Unauthorized sender")]
    UnauthorizedSender,
    #[msg("Invalid port identifier")]
    InvalidPortIdentifier,
    #[msg("Invalid timeout timestamp")]
    InvalidTimeoutTimestamp,
    #[msg("Invalid timeout duration")]
    InvalidTimeoutDuration,
    #[msg("Packet commitment already exists")]
    PacketCommitmentAlreadyExists,
    #[msg("Packet commitment mismatch")]
    PacketCommitmentMismatch,
    #[msg("Packet should have payload")]
    PacketNoPayload,
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
}
