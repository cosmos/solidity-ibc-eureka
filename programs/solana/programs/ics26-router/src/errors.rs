use anchor_lang::prelude::*;

#[error_code]
pub enum IbcRouterError {
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
}

