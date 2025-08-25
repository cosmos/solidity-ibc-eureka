use alloy::sol_types::Error as AbiError;
use std::fmt::Debug;
use thiserror::Error;
use tonic::{Code, Status};

#[derive(Debug, Error)]
pub enum AttestorError {
    #[error("Failed to bulid attestor client due to: {0}")]
    ClientConfigError(String),
    #[error("Failed to retrieve data from client due to: {0}")]
    ClientError(String),
    #[error("Failed to decode packet due to: {0}")]
    DecodePacket(#[source] AbiError),
    #[error("Packet commitment found but invalid due to: {reason}")]
    InvalidCommitment {
        /// Why the commitment was invalid
        reason: String,
    },
    #[error("Failed to bulid signer due to: {0}")]
    SignerConfigError(String),
    #[error("Failed to sign attestation due to: {0}")]
    SignerError(String),
    #[error("Failed to bulid attestor server due to: {0}")]
    ServerConfigError(String),
}

impl From<AttestorError> for Status {
    fn from(value: AttestorError) -> Self {
        Status::new(Code::Internal, value.to_string())
    }
}
