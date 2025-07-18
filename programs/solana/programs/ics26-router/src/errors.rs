use anchor_lang::prelude::*;

#[error_code]
pub enum RouterError {
    #[msg("Unauthorized sender")]
    UnauthorizedSender,
    #[msg("Port already exists")]
    PortAlreadyExists,
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
    #[msg("Already initialized")]
    AlreadyInitialized,
    #[msg("Router not initialized")]
    RouterNotInitialized,
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
}

